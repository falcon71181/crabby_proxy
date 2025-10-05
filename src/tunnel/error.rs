use std::io;
use thiserror::Error;

/// Defines errors that can occur during tunnel operations
#[derive(Debug, Error)]
pub enum TunnelError {
    /// Failed to bind to tunnel port
    #[error("Failed to bind to port {0}: {1}")]
    BindError(u16, String),

    /// Failed to connect to client for tunneling
    #[error("Failed to connect to client {0}: {1}")]
    ClientConnectionError(String, String),

    /// Port allocation failure
    #[error("Port allocation failed: {0}")]
    AllocationError(String),

    /// Requested port is already in use
    #[error("Port {0} is already in use by another tunnel")]
    PortInUse(u16),

    /// Requested port is outside allowed range
    #[error("Port {0} is outside allowed range ({1}-{2})")]
    PortOutOfRange(u16, u16, u16),

    /// No ports available in allocation range
    #[error("No available ports in range {0}-{1}")]
    NoPortsAvailable(u16, u16),

    /// Tunnel not found when attempting to close
    #[error("Tunnel on port {0} not found")]
    TunnelNotFound(u16),

    /// Failed to release tunnel port
    #[error("Failed to release port {0}: {1}")]
    PortReleaseError(u16, String),

    /// Error during tunnel data transfer
    #[error("Tunnel data transfer failed: {0}")]
    DataTransferError(String),

    /// I/O error during tunnel operation
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// General tunnel operation failure
    #[error("Tunnel operation failed: {0}")]
    OperationFailed(String),
}

// Convert PortAllocError to TunnelError
impl From<super::port_allocator::PortAllocError> for TunnelError {
    fn from(err: super::port_allocator::PortAllocError) -> Self {
        match err {
            super::port_allocator::PortAllocError::PortOutOfRange(port, min, max) => {
                TunnelError::PortOutOfRange(port, min, max)
            }
            super::port_allocator::PortAllocError::PortUnavailable(port) => {
                TunnelError::PortInUse(port)
            }
            super::port_allocator::PortAllocError::NoPortsAvailable(min, max) => {
                TunnelError::NoPortsAvailable(min, max)
            }
            e => TunnelError::AllocationError(e.to_string()),
        }
    }
}
