//! Helpers the screens' headless tests share.

/// A desktop that answers one request with `response`; joining it gives
/// the request line ("POST /api/v1/…") and the body.
pub fn fake_desktop(response: String) -> (u16, std::thread::JoinHandle<(String, String)>) {
    let (port, server) = fake_desktop_answers(vec![(200, response)]);
    (
        port,
        std::thread::spawn(move || server.join().unwrap().remove(0)),
    )
}

/// A desktop that answers each request in turn with `(status, body)`.
pub fn fake_desktop_answers(
    answers: Vec<(u16, String)>,
) -> (u16, std::thread::JoinHandle<Vec<(String, String)>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        answers
            .into_iter()
            .map(|(status, response)| {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                let request = request.split(' ').take(2).collect::<Vec<_>>().join(" ");
                (request, String::from_utf8(body).unwrap())
            })
            .collect()
    });
    (port, server)
}
