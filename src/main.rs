use clap::{Parser, Subcommand};
use colored::*;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

// Структура для аргументов командной строки
#[derive(Parser)]
#[command(name = "rust-chat")]
#[command(about = "Простой TCP чат", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Подкоманды (сервер или клиент)
#[derive(Subcommand)]
enum Commands {
    /// Запустить сервер
    Server {
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: String,
    },
    /// Запустить клиент
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

// --- СЕРВЕР ---
async fn run_server(addr: String) -> Result<(), Box<dyn std::error::Error>> {
    // Создаем TCP слушатель
    let listener = TcpListener::bind(&addr).await?;

    // Создаем канал для рассылки сообщений всем клиентам
    let (sender, _receiver) = broadcast::channel::<(String, SocketAddr)>(16);

    println!("{} Сервер запущен на {}", "✓".green(), addr);

    loop {
        // Ждем новое подключение
        let (socket, client_addr) = listener.accept().await?;
        println!("{} Новый клиент: {}", "→".blue(), client_addr);

        // Клонируем отправитель для нового клиента
        let client_sender = sender.clone();
        // Создаем подписчика для этого клиента
        let mut client_receiver = sender.subscribe();

        // Запускаем обработку клиента в отдельной задаче
        tokio::spawn(async move {
            // Разделяем сокет на чтение и запись
            let (read_half, mut write_half) = socket.into_split();

            // Создаем буферизированного читателя
            let reader = BufReader::new(read_half);
            let mut lines = reader.lines();

            loop {
                tokio::select! {
                    // Читаем строку от клиента
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(message)) => {
                                // Отправляем сообщение всем клиентам
                                let _ = client_sender.send((message, client_addr));
                            }
                            _ => break, // Клиент отключился
                        }
                    }

                    // Получаем сообщение для отправки клиенту
                    message = client_receiver.recv() => {
                        match message {
                            Ok((msg, sender_addr)) => {
                                // Не отправляем сообщение обратно отправителю
                                if sender_addr != client_addr {
                                    let _ = write_half.write_all(msg.as_bytes()).await;
                                    let _ = write_half.write_all(b"\n").await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            println!("{} Клиент отключился: {}", "✗".red(), client_addr);
        });
    }
}

// --- КЛИЕНТ ---
async fn run_client(addr: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
    // Подключаемся к серверу
    let socket = TcpStream::connect(&addr).await?;
    println!("{} Подключен к {}", "✓".green(), addr);

    // Разделяем сокет на чтение и запись
    let (read_half, mut write_half) = socket.into_split();

    // Создаем читатель для сокета
    let reader = BufReader::new(read_half);
    let mut lines_from_server = reader.lines();

    // Создаем читатель для ввода с клавиатуры
    let stdin_reader = BufReader::new(tokio::io::stdin());
    let mut lines_from_user = stdin_reader.lines();

    println!("Введите сообщения (Ctrl+C для выхода):");

    loop {
        tokio::select! {
            // Читаем сообщение от сервера
            server_line = lines_from_server.next_line() => {
                match server_line {
                    Ok(Some(message)) => {
                        // Если в сообщении есть упоминание нашего имени
                        if message.contains(&format!("@{}", name)) {
                            println!("{}", message.yellow().bold());
                        } else {
                            println!("{}", message);
                        }
                    }
                    _ => {
                        println!("Сервер отключился");
                        break;
                    }
                }
            }

            // Читаем ввод пользователя
            user_line = lines_from_user.next_line() => {
                match user_line {
                    Ok(Some(message)) => {
                        // Формируем и отправляем сообщение
                        let full_message = format!("{}: {}", name.cyan(), message);
                        let _ = write_half.write_all(full_message.as_bytes()).await;
                        let _ = write_half.write_all(b"\n").await;
                    }
                    _ => break,
                }
            }
        }
    }

    Ok(())
}