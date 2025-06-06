use crate::ErrorBox;

#[derive(Default)]
pub struct Als {}

impl Als {
    pub async fn get(&self) -> Result<String, ErrorBox> {
        Ok("none".to_string())
    }
}
