use bytes::*;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod serialize;

fn pop(barry: &[u8]) -> [u8; 4] {
    barry.try_into().expect("slice with incorrect length")
}
type Error = Box<dyn std::error::Error>;
type Result<T> = core::result::Result<T, Error>;


async fn handle_client(mut socket: TcpStream, id: u32) -> Result<()>{
    println!("Just spanwed thread {id}");

    // In a loop, read data from the socket and write the data back.
    loop {
        let mut buf = BytesMut::with_capacity(1024);

        let read_result = socket.read_buf(&mut buf).await;
        let mut buf = buf.freeze();

        let n = match read_result {
            // socket closed
            Ok(n) if n == 0 => {
                println!("Just killed thread {id}");
                return Ok(());
            },
            Ok(n) => {
                println!("Got {n} bytes!");
                buf[0..n].iter().for_each(|b| print!("{} ", b));

                println!("");

                let username = serialize::read_string(&mut buf);

                println!("str len = {}", username.len());
                println!("str: {}", username);
                n
            },
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                return Ok(());
            }
        };

        let response = "Welcome to the server!";

        let mut resp_buf = BytesMut::new();

        serialize::write_string(&mut resp_buf, response);

        socket.write_all(&resp_buf).await.expect("Failed to write response");
        println!("Just wrote response! {response}");
        socket.flush().await.expect("Failed to flush");

    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let addr = "127.0.0.1:8083";
    let listener = TcpListener::bind(addr).await?;
    println!("Bound to {addr}");

    let mut i = 0;
    loop {
        let (socket, _) = listener.accept().await?;
        i += 1;

        println!("Accepted a socket conn");
        tokio::spawn(async move {
            let _result = handle_client(socket, i).await;
        });
    }
}