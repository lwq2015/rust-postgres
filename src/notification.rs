//! Asynchronous notifications.

use fallible_iterator::{FallibleIterator, IntoFallibleIterator};
use std::fmt;
use std::time::Duration;

use {desynchronized, Result, Connection, NotificationsNew};
use message::Backend;
use error::Error;

/// An asynchronous notification.
#[derive(Clone, Debug)]
pub struct Notification {
    /// The process ID of the notifying backend process.
    pub process_id: i32,
    /// The name of the channel that the notify has been raised on.
    pub channel: String,
    /// The "payload" string passed from the notifying process.
    pub payload: String,
}

/// Notifications from the Postgres backend.
pub struct Notifications<'conn> {
    conn: &'conn Connection,
}

impl<'a> fmt::Debug for Notifications<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Notifications")
            .field("pending", &self.len())
            .finish()
    }
}

impl<'conn> Notifications<'conn> {
    /// Returns the number of pending notifications.
    pub fn len(&self) -> usize {
        self.conn.conn.borrow().notifications.len()
    }

    /// Determines if there are any pending notifications.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a fallible iterator over pending notifications.
    ///
    /// # Note
    ///
    /// This iterator may start returning `Some` after previously returning
    /// `None` if more notifications are received.
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        Iter { conn: self.conn }
    }

    /// Returns a fallible iterator over notifications that blocks until one is
    /// received if none are pending.
    ///
    /// The iterator will never return `None`.
    pub fn blocking_iter<'a>(&'a self) -> BlockingIter<'a> {
        BlockingIter { conn: self.conn }
    }

    /// Returns a fallible iterator over notifications that blocks for a limited
    /// time waiting to receive one if none are pending.
    ///
    /// # Note
    ///
    /// This iterator may start returning `Some` after previously returning
    /// `None` if more notifications are received.
    pub fn timeout_iter<'a>(&'a self, timeout: Duration) -> TimeoutIter<'a> {
        TimeoutIter {
            conn: self.conn,
            timeout: timeout,
        }
    }
}

impl<'a, 'conn> IntoFallibleIterator for &'a Notifications<'conn> {
    type Item = Notification;
    type Error = Error;
    type IntoIter = Iter<'a>;

    fn into_fallible_iterator(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'conn> NotificationsNew<'conn> for Notifications<'conn> {
    fn new(conn: &'conn Connection) -> Notifications<'conn> {
        Notifications { conn: conn }
    }
}

/// A fallible iterator over pending notifications.
pub struct Iter<'a> {
    conn: &'a Connection,
}

impl<'a> FallibleIterator for Iter<'a> {
    type Item = Notification;
    type Error = Error;

    fn next(&mut self) -> Result<Option<Notification>> {
        let mut conn = self.conn.conn.borrow_mut();

        if let Some(notification) = conn.notifications.pop_front() {
            return Ok(Some(notification));
        }

        if conn.is_desynchronized() {
            return Err(Error::Io(desynchronized()));
        }

        match conn.read_message_with_notification_nonblocking() {
            Ok(Some(Backend::NotificationResponse { process_id, channel, payload })) => {
                Ok(Some(Notification {
                    process_id: process_id,
                    channel: channel,
                    payload: payload,
                }))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(Error::Io(err)),
            _ => unreachable!(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.conn.conn.borrow().notifications.len(), None)
    }
}

/// An iterator over notifications which will block if none are pending.
pub struct BlockingIter<'a> {
    conn: &'a Connection,
}

impl<'a> FallibleIterator for BlockingIter<'a> {
    type Item = Notification;
    type Error = Error;

    fn next(&mut self) -> Result<Option<Notification>> {
        let mut conn = self.conn.conn.borrow_mut();

        if let Some(notification) = conn.notifications.pop_front() {
            return Ok(Some(notification));
        }

        if conn.is_desynchronized() {
            return Err(Error::Io(desynchronized()));
        }

        match conn.read_message_with_notification() {
            Ok(Backend::NotificationResponse { process_id, channel, payload }) => {
                Ok(Some(Notification {
                    process_id: process_id,
                    channel: channel,
                    payload: payload,
                }))
            }
            Err(err) => Err(Error::Io(err)),
            _ => unreachable!(),
        }
    }
}

/// An iterator over notifications which will block for a period of time if
/// none are pending.
pub struct TimeoutIter<'a> {
    conn: &'a Connection,
    timeout: Duration,
}

impl<'a> FallibleIterator for TimeoutIter<'a> {
    type Item = Notification;
    type Error = Error;

    fn next(&mut self) -> Result<Option<Notification>> {
        let mut conn = self.conn.conn.borrow_mut();

        if let Some(notification) = conn.notifications.pop_front() {
            return Ok(Some(notification));
        }

        if conn.is_desynchronized() {
            return Err(Error::Io(desynchronized()));
        }

        match conn.read_message_with_notification_timeout(self.timeout) {
            Ok(Some(Backend::NotificationResponse { process_id, channel, payload })) => {
                Ok(Some(Notification {
                    process_id: process_id,
                    channel: channel,
                    payload: payload,
                }))
            }
            Ok(None) => Ok(None),
            Err(err) => Err(Error::Io(err)),
            _ => unreachable!(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.conn.conn.borrow().notifications.len(), None)
    }
}
