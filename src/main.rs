#![feature(io_error_more, let_chains)]

use std::collections::HashMap;
use std::fs::{ metadata as get_metadata, read };
use std::io::{ Write, BufReader, BufRead, self };
use std::net::{ TcpListener, TcpStream, IpAddr };
use anyhow::{ Result, ensure, anyhow, bail };

const PRINT_HEADERS: bool = false;

fn main() -> Result<()> {
	let listener = TcpListener::bind("0.0.0.0:8080")?;

	println!("Connect via http://localhost:8080/");

	for stream in listener.incoming() {
		if let Err(error) = stream.and_then(|mut stream| {
			if let Err(error) = handle_request(&stream) {
				eprintln!("Error: {error:?}");

				match read("web/500.html") {
					Ok(content) => {
						let content_length = content.len();

						write!(stream, "\
							500 Internal Server Error\r\n\
							Server: apex\r\n\
							Content-Length: {content_length}\r\n\
							\r\n"
						)?;

						stream.write_all(&content)?;
					}

					Err(error) => {
						if error.kind() != io::ErrorKind::NotFound {
							return Err(error);
						}

						stream.write_all(b"\
							500 Internal Server Error\r\n\
							Server: apex\r\n\
							Content-Length: 25\r\n\
							Content-Type: text/plain\r\n\
							\r\n\
							500 Internal Server Error"
						)?;
					}
				}
			}

			return Ok(());
		}) {
			eprintln!("Error: {error:?}");
		}
	}

	return Ok(())
}

fn handle_request(mut stream: &TcpStream) -> Result<()> {
	let mut request = BufReader::new(stream).lines();
	let first_line = request.next().ok_or(anyhow!("Option was None"))??;

	if !first_line.starts_with("GET ") {
		stream.write_all(b"\
			HTTP/1.0 400 Bad Request\r\n\
			Server: apex\r\n\
			Content-Type: text/plain\r\n\
			Content-Length: 15\r\n\
			\r\n\
			400 Bad Request"
		)?;

		return Ok(());
	}

	if first_line.ends_with(" HTTP/1.0") {
		stream.write_all(b"HTTP/1.0 ")?;
	} else if first_line.ends_with(" HTTP/1.1") {
		stream.write_all(b"HTTP/1.1 ")?;
	} else {
		stream.write_all(b"400 Bad Request\r\n\r\n")?;
		return Ok(());
	}

	let url_path = &first_line[4 .. (first_line.len() - 9)];
	let mut headers = HashMap::new();

	for line in request {
		let line = line?;

		if line.is_empty() {
			break
		}

		let colon_index = line.find(':').ok_or(anyhow!("Option was None"))?;

		headers.insert(line[0 .. colon_index].to_lowercase(), line[colon_index + 1 + (line.as_bytes()[colon_index + 1] == b' ') as usize ..].to_owned());
	}

	if let Some(hostname) = headers.get("host") {
		let address = stream.peer_addr()?;

		if let IpAddr::V4(ip) = address.ip() && ip.is_private() && let Some(real_ip) = headers.get("x-real-ip") {
			let port = address.port();

			if PRINT_HEADERS {
				println!("{real_ip}:{port} GET {hostname}{} {headers:#?}", &first_line[4 ..]);
			} else {
				println!("{real_ip}:{port} GET {hostname}{}", &first_line[4 ..]);
			}
		} else {
			if PRINT_HEADERS {
				println!("{address} GET {hostname}{} {headers:#?}", &first_line[4 ..]);
			} else {
				println!("{address} GET {hostname}{}", &first_line[4 ..]);
			}
		}

		let mut file_path = format!("web/{hostname}{url_path}");

		match get_metadata(&file_path) {
			Ok(metadata) => {
				if metadata.is_dir() && !url_path.ends_with("/") {
					match get_metadata(format!("{file_path}/index.html")) {
						Ok(metadata) => {
							if metadata.is_file() {
								let content_length = 31 + hostname.len() + url_path.len();

								write!(stream, "\
									301 Moved Permanently\r\n\
									Server: apex\r\n\
									Content-Type: text/plain\r\n\
									Content-Length: {content_length}\r\n\
									Location: {url_path}/\r\n\
									\r\n\
									301 Moved Permanently\r\n\
									http://{hostname}{url_path}/"
								)?;

								return Ok(());
							}
						}

						Err(error) => {
							if error.kind() != io::ErrorKind::NotFound {
								bail!(error);
							}
						}
					}
				} else {
					if metadata.is_file() {
						ensure!(!file_path.ends_with("/"));
					}

					if file_path.ends_with("/") {
						file_path += "index.html";
					}

					match read(file_path) {
						Ok(content) => {
							let content_length = content.len();

							write!(stream, "200 OK\r\nServer: apex\r\nContent-Length: {content_length}\r\n\r\n")?;
							stream.write_all(&content)?;

							return Ok(());
						}

						Err(error) => {
							if error.kind() != io::ErrorKind::NotFound {
								bail!(error);
							}
						}
					}
				}
			}

			Err(error) => {
				match error.kind() {
					io::ErrorKind::NotADirectory => {
						let url_path = &url_path[0 .. url_path.len() - 1];
						let content_length = 30 + hostname.len() + url_path.len();

						write!(stream, "\
							301 Moved Permanently\r\n\
							Server: apex\r\n\
							Location: {url_path}\r\n\
							Content-Length: {content_length}\r\n\
							Content-Type: text/plain\r\n\
							\r\n\
							301 Moved Permanently\r\n\
							http://{hostname}{url_path}"
						)?;

						return Ok(());
					}

					io::ErrorKind::NotFound => {}
					_ => bail!(error)
				}

			}
		}
	}

	if let Ok(content) = read("web/404.html") {
		write!(stream, "404 Not Found\r\nServer: apex\r\nContent-Length: {}\r\n\r\n", content.len())?;
		stream.write_all(&content)?;
	}

	stream.write_all(b"\
		404 Not Found\r\n\
		Server: apex\r\n\
		Content-Length: 13\r\n\
		Content-Type: text/plain\r\n\
		\r\n\
		404 Not Found"
	)?;

	return Ok(());
}
