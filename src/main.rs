use postgres::{ Client, NoTls };
use postgres::Error as PostgresError;
use std::net::{ TcpListener, TcpStream };
use std::io::{ Read, Write };
use std::env;

#[macro_use]
extern crate serde_derive;

//Model: Car struct with id, brand, model, year, price
#[derive(Serialize, Deserialize)]
struct Car {
    id: Option<i32>,
    brand: String,
    model: String,
    year: i32,
    price: f64,
}

//DATABASE URL
const DB_URL: &str = env!("DATABASE_URL");

//constants
const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
const INTERNAL_ERROR: &str = "HTTP/1.1 500 INTERNAL ERROR\r\n\r\n";

//main function
fn main() {
    //Set Database
    match set_database() {
        Ok(_) => println!("Database setup successful"),
        Err(e) => eprintln!("Database setup failed: {}", e),
    }

    //start server and print port
    let listener = TcpListener::bind(format!("0.0.0.0:6001")).unwrap();
    println!("Server listening on port 6001");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_client(stream);
            }
            Err(e) => {
                println!("Unable to connect: {}", e);
            }
        }
    }
}

//handle requests
fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    let mut request = String::new();

    match stream.read(&mut buffer) {
        Ok(size) => {
            request.push_str(String::from_utf8_lossy(&buffer[..size]).as_ref());

            let (status_line, content) = match &*request {
                r if r.starts_with("POST /cars") => handle_post_request(r),
                r if r.starts_with("GET /cars/") => handle_get_request(r),
                r if r.starts_with("GET /cars") => handle_get_all_request(r),
                r if r.starts_with("PUT /cars/") => handle_put_request(r),
                r if r.starts_with("DELETE /cars/") => handle_delete_request(r),
                _ => (NOT_FOUND.to_string(), "404 not found".to_string()),
            };

            stream.write_all(format!("{}{}", status_line, content).as_bytes()).unwrap();
        }
        Err(e) => eprintln!("Unable to read stream: {}", e),
    }
}

//handle post request
fn handle_post_request(request: &str) -> (String, String) {
    match (get_car_request_body(&request), Client::connect(&*DB_URL, NoTls)) {
        (Ok(car), Ok(mut client)) => {
            client
                .execute(
                    "INSERT INTO cars (brand, model, year, price) VALUES ($1, $2, $3, $4)",
                    &[&car.brand, &car.model, &car.year, &car.price]
                )
                .unwrap();

            (OK_RESPONSE.to_string(), "Car created".to_string())
        }
        _ => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

//handle get request
fn handle_get_request(request: &str) -> (String, String) {
    match (get_id(&request).parse::<i32>(), Client::connect(&*DB_URL, NoTls)) {
        (Ok(id), Ok(mut client)) =>
            match client.query_one("SELECT * FROM cars WHERE id = $1", &[&id]) {
                Ok(row) => {
                    let car = Car {
                        id: row.get(0),
                        brand: row.get(1),
                        model: row.get(2),
                        year: row.get(3),
                        price: row.get(4),
                    };

                    (OK_RESPONSE.to_string(), serde_json::to_string(&car).unwrap())
                }
                _ => (NOT_FOUND.to_string(), "Car not found".to_string()),
            }

        _ => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

//handle get all request
fn handle_get_all_request(_request: &str) -> (String, String) {
    match Client::connect(&*DB_URL, NoTls) {
        Ok(mut client) => {
            let mut cars = Vec::new();

            for row in client.query("SELECT id, brand, model, year, price FROM cars", &[]).unwrap() {
                cars.push(Car {
                    id: row.get(0),
                    brand: row.get(1),
                    model: row.get(2),
                    year: row.get(3),
                    price: row.get(4),
                });
            }

            (OK_RESPONSE.to_string(), serde_json::to_string(&cars).unwrap())
        }
        _ => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

//handle put request
fn handle_put_request(request: &str) -> (String, String) {
    match
        (
            get_id(&request).parse::<i32>(),
            get_car_request_body(&request),
            Client::connect(&*DB_URL, NoTls),
        )
    {
        (Ok(id), Ok(car), Ok(mut client)) => {
            client
                .execute(
                    "UPDATE cars SET brand = $1, model = $2, year = $3, price = $4 WHERE id = $5",
                    &[&car.brand, &car.model, &car.year, &car.price, &id]
                )
                .unwrap();

            (OK_RESPONSE.to_string(), "Car updated".to_string())
        }
        _ => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

//handle delete request
fn handle_delete_request(request: &str) -> (String, String) {
    match (get_id(&request).parse::<i32>(), Client::connect(&*DB_URL, NoTls)) {
        (Ok(id), Ok(mut client)) => {
            let rows_affected = client.execute("DELETE FROM cars WHERE id = $1", &[&id]).unwrap();

            //if rows affected is 0, car not found
            if rows_affected == 0 {
                return (NOT_FOUND.to_string(), "Car not found".to_string());
            }

            (OK_RESPONSE.to_string(), "Car deleted".to_string())
        }
        _ => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

//db setup
fn set_database() -> Result<(), PostgresError> {
    let mut client = Client::connect(&*DB_URL, NoTls)?;
    client.batch_execute(
        "
        CREATE TABLE IF NOT EXISTS cars (
            id SERIAL PRIMARY KEY,
            brand VARCHAR NOT NULL,
            model VARCHAR NOT NULL,
            year INT NOT NULL,
            price FLOAT NOT NULL
        )
    "
    )?;
    Ok(())
}

//Get id from request URL
fn get_id(request: &str) -> &str {
    request.split("/").nth(2).unwrap_or_default().split_whitespace().next().unwrap_or_default()
}

//deserialize car from request body without id
fn get_car_request_body(request: &str) -> Result<Car, serde_json::Error> {
    serde_json::from_str(request.split("\r\n\r\n").last().unwrap_or_default())
}
