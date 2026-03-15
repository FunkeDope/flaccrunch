use md5::{Digest, Md5};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Compute the MD5 hash of the decoded audio stream from a FLAC file.
/// This spawns `flac -d -c -s --force-raw-format --endian=little --sign=signed -- <file>`
/// and computes the MD5 of the raw PCM output.
pub async fn hash_decoded_audio(
    flac_bin: &Path,
    file_path: &Path,
) -> Result<String, String> {
    let mut cmd = Command::new(flac_bin);
    cmd.args([
        "-d",                // Decode
        "-c",                // Write to stdout
        "-s",                // Silent (no progress)
        "--force-raw-format",
        "--endian=little",
        "--sign=signed",
        "--",
    ]);
    cmd.arg(file_path);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn flac decoder: {e}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture flac stdout".to_string())?;

    // Read stdout and compute MD5 incrementally
    let mut hasher = Md5::new();
    let mut buffer = vec![0u8; 65536]; // 64KB read buffer

    loop {
        let n = stdout
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Error reading decoded audio: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for flac decoder: {e}"))?;

    if !status.success() {
        return Err(format!(
            "FLAC decode failed with exit code: {:?}",
            status.code()
        ));
    }

    let result = hasher.finalize();
    Ok(format!("{:032x}", result))
}
