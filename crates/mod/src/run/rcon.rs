//! A minimal RCON client, enough to send a command to the dev server: a
//! packet is its length, an id, a type and a body ending with two zero bytes,
//! integers in little endian. The server answers a login with the same id, or
//! -1 for a wrong password.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

const LOGIN: i32 = 3;
const COMMAND: i32 = 2;
/// The largest packet the server sends: a 4096-byte body and the header.
const MAX_PACKET: i32 = 4096 + 10;
const TIMEOUT: Duration = Duration::from_secs(10);

/// Sends `command` to the server's RCON on `localhost:port`.
pub fn command(port: u16, password: &str, command: &str) -> io::Result<()> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    send(&mut stream, 1, LOGIN, password)?;
    if receive(&mut stream)? == -1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the server refused the RCON password",
        ));
    }
    send(&mut stream, 2, COMMAND, command)?;
    // `stop` may close the connection before its answer.
    let _ = receive(&mut stream);
    Ok(())
}

fn send(stream: &mut TcpStream, id: i32, kind: i32, body: &str) -> io::Result<()> {
    let length = i32::try_from(body.len() + 10)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "RCON body too long"))?;
    let mut packet = Vec::with_capacity(body.len() + 14);
    packet.extend(length.to_le_bytes());
    packet.extend(id.to_le_bytes());
    packet.extend(kind.to_le_bytes());
    packet.extend(body.as_bytes());
    packet.extend([0, 0]);
    stream.write_all(&packet)
}

/// Reads one packet; returns its id.
fn receive(stream: &mut TcpStream) -> io::Result<i32> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = i32::from_le_bytes(length);
    if !(10..=MAX_PACKET).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("RCON packet of {length} bytes"),
        ));
    }
    let mut rest = vec![0; length as usize];
    stream.read_exact(&mut rest)?;
    Ok(i32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    /// A packet received: id, type, body.
    type Packet = (i32, i32, String);

    /// A server that answers the login with `login_id` and records what it
    /// receives.
    fn server(login_id: Option<i32>) -> (u16, thread::JoinHandle<Vec<Packet>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            loop {
                let mut length = [0; 4];
                if stream.read_exact(&mut length).is_err() {
                    return got;
                }
                let mut rest = vec![0; i32::from_le_bytes(length) as usize];
                stream.read_exact(&mut rest).unwrap();
                let int = |i: usize| i32::from_le_bytes(rest[i..i + 4].try_into().unwrap());
                let (id, kind) = (int(0), int(4));
                got.push((
                    id,
                    kind,
                    String::from_utf8(rest[8..rest.len() - 2].to_vec()).unwrap(),
                ));
                let reply_id = match kind {
                    LOGIN => login_id.unwrap_or(id),
                    _ => id,
                };
                send(&mut stream, reply_id, 0, "").unwrap();
            }
        });
        (port, handle)
    }

    #[test]
    fn logs_in_then_sends_the_command() {
        let (port, handle) = server(None);
        command(port, "secret", "stop").unwrap();
        assert_eq!(
            handle.join().unwrap(),
            [
                (1, LOGIN, "secret".to_owned()),
                (2, COMMAND, "stop".to_owned())
            ]
        );
    }

    #[test]
    fn a_refused_password_sends_nothing() {
        let (port, handle) = server(Some(-1));
        let err = command(port, "wrong", "stop").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(handle.join().unwrap().len(), 1);
    }
}
