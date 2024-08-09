//! # time
//!
//! This module provides primitives for working with time.

use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};
use time::OffsetDateTime;

use crate::device::QBDeviceId;

/// This struct represents a timestamp recorded (maybe conflicts).
#[derive(
    Encode, Decode, Serialize, Deserialize, Clone, Default, Debug, Eq, PartialEq, PartialOrd,
)]
pub struct QBTimeStamp(u64);

impl fmt::Display for QBTimeStamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let datetime = OffsetDateTime::from_unix_timestamp(self.0 as i64).unwrap();
        write!(f, "{}", datetime)
    }
}

/// This struct represents a timestamp recorded on a specific device (no conflicts).
#[derive(
    Encode, Decode, Serialize, Deserialize, Clone, Default, Debug, Eq, PartialEq, PartialOrd,
)]
pub struct QBTimeStampUnique {
    /// The timestamp
    pub timestamp: QBTimeStamp,
    /// The device id
    pub device_id: QBDeviceId,
}

impl Ord for QBTimeStampUnique {
    /// Compare this unique timestamp to another. This should never return
    /// std::cmp::Ordering::Equal for timestamps returned by two seperate invocations
    /// of the [QBTimeStampRecorder::record] method.
    ///
    /// This will compare the structs first by their timestamps, and if those
    /// are equal then by the device_id.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.timestamp.0.cmp(&other.timestamp.0) {
            std::cmp::Ordering::Equal => self.device_id.0.cmp(&other.device_id.0),
            v => v,
        }
    }
}

impl fmt::Display for QBTimeStampUnique {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.device_id, self.timestamp)
    }
}

/// The base timestamp.
pub const QB_TIMESTAMP_BASE: QBTimeStampUnique = QBTimeStampUnique {
    timestamp: QBTimeStamp(0),
    device_id: QBDeviceId(0),
};

/// A timestamp recorder provides the ability to generate 100% unique timestamps.
/// There will never be a conflict.
pub struct QBTimeStampRecorder {
    device_id: QBDeviceId,
    counter: u64,
}

impl From<QBDeviceId> for QBTimeStampRecorder {
    fn from(value: QBDeviceId) -> Self {
        Self::from_device_id(value)
    }
}

impl QBTimeStampRecorder {
    /// Create a timestamp recorder using this device id.
    pub fn from_device_id(device_id: QBDeviceId) -> Self {
        Self {
            device_id,
            counter: 0,
        }
    }

    /// Record a timestamp.
    pub fn record(&mut self) -> QBTimeStampUnique {
        // TODO: switch to instant for monotonically increasing time
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let ts = QBTimeStampUnique {
            timestamp: QBTimeStamp(ts + self.counter),
            device_id: self.device_id.clone(),
        };
        self.counter += 1;
        ts
    }
}
