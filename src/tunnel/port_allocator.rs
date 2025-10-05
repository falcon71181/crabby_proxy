use std::collections::{HashSet, VecDeque};
use thiserror::Error;

/// Custom error types for port allocation
#[derive(Debug, Error)]
pub enum PortAllocError {
    #[error("Port {0} is out of allowed range ({1}-{2})")]
    PortOutOfRange(u16, u16, u16),

    #[error("Port {0} is already allocated")]
    PortUnavailable(u16),

    #[error("No available ports in the range {0}-{1}")]
    NoPortsAvailable(u16, u16),

    #[error("Port {0} is not currently allocated")]
    PortNotAllocated(u16),
}

/// Manages port allocation within a specified range
pub struct PortAllocator {
    min_port: u16,
    max_port: u16,
    allocated_ports: HashSet<u16>,
    available_ports: VecDeque<u16>,
    next_port: u16,
}

impl PortAllocator {
    /// Creates a new PortAllocator with the specified range
    ///
    /// # Arguments
    /// * `min_port` - Minimum port in the range (inclusive)
    /// * `max_port` - Maximum port in the range (inclusive)
    ///
    /// # Panics
    /// Panics if min_port > max_port or if range is empty
    pub fn new(min_port: u16, max_port: u16) -> Self {
        assert!(min_port > 0, "Ports must be > 0");
        assert!(min_port <= max_port, "Invalid port range");

        // Create a deque with all ports in range for O(1) allocation
        let all_ports: VecDeque<u16> = (min_port..=max_port).collect();

        PortAllocator {
            min_port,
            max_port,
            allocated_ports: HashSet::new(),
            available_ports: all_ports,
            next_port: min_port,
        }
    }

    /// Allocates a port, preferring the requested port if available
    ///
    /// # Arguments
    /// * `preferred` - Optional preferred port number
    ///
    /// # Returns
    /// Allocated port number
    pub fn allocate_port(&mut self, preferred: Option<u16>) -> Result<u16, PortAllocError> {
        if let Some(port) = preferred {
            return self.allocate_specific(port);
        }
        self.allocate_next()
    }

    /// Allocates a specific port if available
    ///
    /// # Arguments
    /// * `port` - Port number to allocate
    pub fn allocate_specific(&mut self, port: u16) -> Result<u16, PortAllocError> {
        // Validate port is in range
        if port < self.min_port || port > self.max_port {
            return Err(PortAllocError::PortOutOfRange(
                port,
                self.min_port,
                self.max_port,
            ));
        }

        // Check if port is available
        if self.allocated_ports.contains(&port) {
            return Err(PortAllocError::PortUnavailable(port));
        }

        // Allocate the port
        self.allocated_ports.insert(port);
        self.available_ports.retain(|&p| p != port);

        Ok(port)
    }

    /// Allocates the next available port in the range
    pub fn allocate_next(&mut self) -> Result<u16, PortAllocError> {
        if let Some(port) = self.available_ports.pop_front() {
            self.allocated_ports.insert(port);
            self.next_port = if port == self.max_port {
                self.min_port
            } else {
                port + 1
            };
            return Ok(port);
        }

        Err(PortAllocError::NoPortsAvailable(
            self.min_port,
            self.max_port,
        ))
    }

    /// Releases a previously allocated port
    ///
    /// # Arguments
    /// * `port` - Port number to release
    pub fn release_port(&mut self, port: u16) -> Result<(), PortAllocError> {
        if !self.allocated_ports.contains(&port) {
            return Err(PortAllocError::PortNotAllocated(port));
        }

        // Release the port
        self.allocated_ports.remove(&port);

        // Maintain sorted order when re-adding to available ports
        if port < self.next_port {
            self.available_ports.push_front(port);
        } else {
            self.available_ports.push_back(port);
        }

        Ok(())
    }

    /// Checks if a port is currently allocated
    pub fn is_allocated(&self, port: u16) -> bool {
        self.allocated_ports.contains(&port)
    }

    /// Checks if a port is within the allocator's range
    pub fn is_in_range(&self, port: u16) -> bool {
        port >= self.min_port && port <= self.max_port
    }

    /// Gets the number of available ports
    pub fn available_count(&self) -> usize {
        self.available_ports.len()
    }

    /// Gets the number of allocated ports
    pub fn allocated_count(&self) -> usize {
        self.allocated_ports.len()
    }
}
