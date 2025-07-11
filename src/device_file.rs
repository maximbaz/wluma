use std::io::SeekFrom;

use anyhow::Result;
use smol::fs::File;
use smol::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub async fn read(file: &mut File) -> Result<f64> {
    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    file.seek(SeekFrom::Start(0)).await?;
    Ok(content.trim().parse()?)
}

pub async fn write(file: &mut File, value: f64) -> Result<()> {
    file.write_all(value.to_string().as_bytes()).await?;
    file.seek(SeekFrom::Start(0)).await?;
    Ok(())
}
