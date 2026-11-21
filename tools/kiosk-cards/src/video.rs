//! Ролики товаров: перекодирование через ffmpeg.
//!
//! Айфон снимает в HEVC (H.265) и с большим разрешением. Киоски на
//! Windows 7 HEVC не показывают, а слабым процессорам тяжело тянуть
//! большое видео. Поэтому каждый ролик приводим к одному виду:
//! H.264 (MP4), без звука, не длиннее video_seconds, по длинной стороне
//! не больше video_max_side, 25 кадров/с, «быстрый старт» (moov в начале).
//! Поворот из метаданных айфона ffmpeg применяет сам.
//! Плюс неподвижный кадр (JPEG) — для плитки подборки и как «обложка»,
//! пока ролик грузится.

use std::path::Path;
use std::process::{Command, Stdio};

pub struct VideoOptions {
    pub max_side: u32,
    pub seconds: f64,
    pub boomerang: bool,
}

fn run(ffmpeg: &Path, args: &[String]) -> Result<(), String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — без мелькающих окон
    }
    let out = cmd.output().map_err(|e| format!("не запускается ffmpeg ({}): {e}", ffmpeg.display()))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = err.lines().rev().take(4).collect();
    Err(format!("ffmpeg: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | ")))
}

/// Есть ли рабочий ffmpeg.
pub fn ffmpeg_version(ffmpeg: &Path) -> Option<String> {
    let out = Command::new(ffmpeg).arg("-version").stdin(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).lines().next().map(|s| s.to_string())
}

/// Перекодирует ролик в MP4/H.264 и сохраняет обложку. Пишет во
/// временные файлы и переименовывает в конце — наполовину записанный
/// ролик никогда не окажется на месте готового.
pub fn make_card_video(ffmpeg: &Path, src: &Path, dst_mp4: &Path, dst_jpg: &Path, opt: &VideoOptions) -> Result<(), String> {
    let side = opt.max_side.max(240);
    // ширина/высота чётные (требование H.264), длинная сторона ≤ side
    let scale = format!("scale='if(gte(iw,ih),min({side},iw),-2)':'if(gte(iw,ih),-2,min({side},ih))':flags=lanczos");
    let base = format!("{scale},fps=25,format=yuv420p");
    let secs = format!("{:.2}", opt.seconds.clamp(1.0, 10.0));
    let tmp_mp4 = dst_mp4.with_extension("tmp.mp4");
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into()];
    args.extend(["-t".into(), secs.clone(), "-i".into(), src.display().to_string()]);
    if opt.boomerang {
        args.extend([
            "-filter_complex".into(),
            format!("[0:v]{base},split[a][b];[b]reverse[r];[a][r]concat=n=2:v=1:a=0[v]"),
            "-map".into(),
            "[v]".into(),
        ]);
    } else {
        args.extend(["-vf".into(), base]);
    }
    args.extend([
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-profile:v".into(),
        "main".into(),
        "-preset".into(),
        "slow".into(),
        "-crf".into(),
        "25".into(),
        "-movflags".into(),
        "+faststart".into(),
        tmp_mp4.display().to_string(),
    ]);
    run(ffmpeg, &args)?;

    // Обложка: кадр из середины ролика.
    let tmp_jpg = dst_jpg.with_extension("tmp.jpg");
    let mid = format!("{:.2}", (opt.seconds.clamp(1.0, 10.0) / 2.0).min(1.5));
    let poster_args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        mid,
        "-i".into(),
        tmp_mp4.display().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-q:v".into(),
        "3".into(),
        tmp_jpg.display().to_string(),
    ];
    if run(ffmpeg, &poster_args).is_err() || !tmp_jpg.exists() {
        // очень короткий ролик — берём первый кадр
        let first: Vec<String> = vec![
            "-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into(),
            "-i".into(), tmp_mp4.display().to_string(), "-frames:v".into(), "1".into(),
            "-q:v".into(), "3".into(), tmp_jpg.display().to_string(),
        ];
        run(ffmpeg, &first)?;
    }
    std::fs::rename(&tmp_mp4, dst_mp4).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_jpg, dst_jpg).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg() -> Option<std::path::PathBuf> {
        let p = std::env::var("FFMPEG").map(std::path::PathBuf::from).unwrap_or_else(|_| "ffmpeg".into());
        ffmpeg_version(&p).map(|_| p)
    }

    #[test]
    fn transcodes_portrait_hevc_like_clip_to_small_h264() {
        let Some(ff) = ffmpeg() else {
            eprintln!("ffmpeg не найден — тест пропущен");
            return;
        };
        let dir = std::env::temp_dir().join(format!("kiosk-cards-video-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // «съёмка с айфона»: 1080x1920, 6 секунд — длиннее, чем нужно
        let src = dir.join("in.mov");
        let gen: Vec<String> = [
            "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
            "testsrc2=size=1080x1920:rate=30:duration=6", "-c:v", "libx264", "-pix_fmt", "yuv420p",
        ]
        .iter()
        .map(|s| s.to_string())
        .chain([src.display().to_string()])
        .collect();
        run(&ff, &gen).unwrap();

        for boomerang in [false, true] {
            let mp4 = dir.join(format!("card_1_{boomerang}.mp4"));
            let jpg = dir.join(format!("card_1_{boomerang}.jpg"));
            make_card_video(&ff, &src, &mp4, &jpg, &VideoOptions { max_side: 720, seconds: 3.0, boomerang }).unwrap();
            assert!(mp4.metadata().unwrap().len() > 1000);
            let img = image::open(&jpg).unwrap();
            assert_eq!(img.height(), 720, "длинная сторона уменьшена до 720");
            assert_eq!(img.width(), 406, "пропорции сохранены, ширина чётная");
        }
    }
}
