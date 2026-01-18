use anyhow::Result;

pub trait Output {
    fn line(&mut self, message: impl Into<String>) -> Result<()>;
}
