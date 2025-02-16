use tokio::{
    io::{BufReader, AsyncWriteExt, split},
    net::TcpListener,
};
use tokio_stream::StreamExt;
use prk_async_dataflow::AsyncJsonParser;
use serde::{Deserialize, Serialize};

#[derive(Deserialize,Serialize ,Debug)]
struct ChatMessage {
    username: String,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind the TCP listener to a local address.
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    loop {
        // Accept an incoming connection.
        let (socket, addr) = listener.accept().await?;
        println!("Accepted connection from: {}", addr);

        // Spawn a new task to handle each connection concurrently.
        tokio::spawn(async move {
            // Split the socket into a read half and a write half.
            let (read_half, mut write_half) = split(socket);
            // Wrap the read half with a BufReader for efficient buffering.
            let reader = BufReader::new(read_half);
            // Create an instance of the AsyncJsonParser to extract JSON messages.
            let  parser = AsyncJsonParser::new(reader);
            // Convert the parser into an asynchronous stream of ChatMessage items.
            let mut json_stream = parser.into_stream::<ChatMessage>();

            // Process each JSON message as it arrives.
            while let Some(result) = json_stream.next().await {
                match result {
                    Ok(chat_msg) => {
                        println!("Received message from {}: {:?}", addr, chat_msg);
                        // Create an acknowledgment message.
                        let ack = format!("ACK: Received your message '{}'\n", chat_msg.message);
                        // Send the acknowledgment back to the client.
                        if let Err(e) = write_half.write_all(ack.as_bytes()).await {
                            eprintln!("Error sending ACK to {}: {}", addr, e);
                            break;
                        }
                        // Optionally, flush the writer to ensure immediate transmission.
                        if let Err(e) = write_half.flush().await {
                            eprintln!("Error flushing writer for {}: {}", addr, e);
                        }
                    }
                    Err(err) => {
                        eprintln!("Error parsing JSON from {}: {}", addr, err);
                        // Optionally break the loop to close the connection on error.
                        break;
                    }
                }
            }
            println!("Connection with {} closed.", addr);
        });
    }
}
