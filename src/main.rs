use clap::{Parser, Subcommand};
use colored::*;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

#[derive(Parser)]
#[command(name = "rust-chat-app")]
#[command(about = "A simple TCP chat with Server and Client modes", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Запуск сервера
    Server {
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: String,
    },
    /// Запуск клиента
    Client {
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: String,
        #[arg(short, long)]
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server { addr } => run_server(addr).await?,
        Commands::Client { addr, name } => run_client(addr, name).await?,
    }

    Ok(())
}

// --- СЕРВЕРНАЯ ЛОГИКА ---

async fn run_server(addr: String) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(&addr).await?;
    // Канал широковещательной рассылки (16 сообщений в буфере)
    let (tx, _rx) = broadcast::channel::<(String, SocketAddr)>(16);

    println!("{} сервер запущен на {}", " INFO ".on_green().black(), addr);

    loop {
        let (socket, addr) = listener.accept().await?;
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        println!("{} новое подключение: {}", " CONN ".on_blue().black(), addr);

        tokio::spawn(async move {
            let (reader, writer) = socket.into_split();
            let mut lines_reader = FramedRead::new(reader, LinesCodec::new());
            let mut lines_writer = FramedWrite::new(writer, LinesCodec::new());

            loop {
                tokio::select! {
                    // Читаем сообщение от клиента и отправляем всем остальным
                    result = lines_reader.next() => {
                        match result {
                            Some(Ok(msg)) => {
                                let _ = tx.send((msg, addr));
                            }
                            _ => break, // Ошибка или дисконнект
                        }
                    }
                    // Получаем сообщение из канала рассылки и пишем клиенту
                    result = rx.recv() => {
                        match result {
                            Ok((msg, other_addr)) => {
                                if addr != other_addr {
                                    if let Err(_) = lines_writer.send(msg).await {
                                        break;
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                }
            }
            println!("{} клиент отключился: {}", " DISC ".on_red().black(), addr);
        });
    }
}

// --- КЛИЕНТСКАЯ ЛОГИКА ---

async fn run_client(addr: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
    let socket = TcpStream::connect(&addr).await?;
    println!("{} Подключено к {}", " OK ".on_green().black(), addr);

    let (reader, writer) = socket.into_split();
    let mut lines_reader = FramedRead::new(reader, LinesCodec::new());
    let mut lines_writer = FramedWrite::new(writer, LinesCodec::new());

    let my_tag = format!("@{}", name);
    let mut stdin_reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());

    loop {
        tokio::select! {
            // Читаем входящие сообщения от сервера
            result = lines_reader.next() => {
                match result {
                    Some(Ok(msg)) => {
                        if msg.contains(&my_tag) {
                            // Подсвечиваем сообщение, если в нем упомянут пользователь
                            println!("{}", msg.yellow().bold());
                        } else {
                            println!("{}", msg);
                        }
                    }
                    _ => {
                        println!("Соединение с сервером потеряно.");
                        break;
                    }
                }
            }
            // Читаем ввод пользователя из консоли и отправляем на сервер
            result = stdin_reader.next() => {
                match result {
                    Some(Ok(content)) => {
                        let full_msg = format!("{}: {}", name.bright_cyan(), content);
                        if let Err(_) = lines_writer.send(full_msg).await {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
    }

    Ok(())
}