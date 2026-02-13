use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Server already connected: {0}")]
    AlreadyConnected(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("MCP protocol error: {0}")]
    Protocol(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Authentication required: {0}")]
    AuthRequired(String),

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Integration not found: {0}")]
    IntegrationNotFound(String),

    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
