use crate::utils::protocol::Frame;
use anyhow::Result;

/// Utility for reading framed messages from a stream buffer
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    /// Creates a new frame reader with an empty buffer
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Adds new data to the internal buffer
    pub fn feed_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Attempts to read a complete frame from the buffer
    /// Returns None if there's insufficient data for a complete frame
    pub fn try_read_frame(&mut self) -> Result<Option<Frame>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }

        // Read frame length
        let length = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;

        // Check if we have the complete frame
        if self.buffer.len() < 4 + length {
            return Ok(None);
        }

        // Extract frame data
        let frame_data = &self.buffer[..4 + length];
        let (frame, _) = Frame::deserialize(frame_data)
            .map_err(|e| anyhow::anyhow!("Frame deserialization error: {}", e))?;

        // Remove processed data from buffer
        self.buffer.drain(..4 + length);

        Ok(Some(frame))
    }

    /// Clears the internal buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
