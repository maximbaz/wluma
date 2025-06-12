use anyhow::Result;

#[derive(Default)]
pub struct Als {}

impl Als {
    pub async fn get(&self) -> Result<String> {
        Ok("none".to_string())
    }
}
