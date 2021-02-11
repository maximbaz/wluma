use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub fn read(file: &mut File) -> Result<f64, Box<dyn Error>> {
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(content.trim().parse()?)
}

pub fn write(file: &mut File, value: f64) -> Result<(), Box<dyn Error>> {
    file.write_all(value.to_string().as_bytes())?;
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}
