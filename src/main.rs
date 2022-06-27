use std::io::prelude::*;
use std::net::TcpListener;
use std::{process, thread};
use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use signal_hook::{consts, iterator::Signals};
use prometheus::{Opts, Gauge, Registry, TextEncoder, Encoder};
use paho_mqtt as mqtt;

struct Metrics {
    registry: Registry,

    metrics_map: HashMap<String, Gauge>,
}

impl Metrics {
    fn new() -> Metrics {
        // TODO: rewrite with generics, and set by string-name

        Metrics {
            registry: Registry::new(),
            metrics_map: HashMap::new(),
        }
    }

    fn register_metric(&mut self, name: &str, desc: &str) {
        let metric = Gauge::with_opts(Opts::new(name, desc))
            .unwrap();

        self.registry.register(Box::new(metric.clone())).unwrap();
        self.metrics_map.insert(
            name.to_string(),
            metric,
        );

    }

    fn set_metric(&mut self, name: &str, val: f64) {
        match self.metrics_map.get(name) {
          Some(metric) => metric.set(val),
          None => {}
        };
    }

    fn get_metrics(&self) -> String {
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        String::from_utf8(buffer).unwrap()
    }
}

fn run_consuming(m: Arc<Mutex<Metrics>>) {
    let subscriptions = [
        "metrics/temperature",
        "metrics/humidity",
        "metrics/pressure",
        "metrics/smoke",
        "metrics/propane",
        "metrics/methane"
    ];
    let qos = [
        1,
        1,
        1,
        1,
        1,
        1,
    ];

    let create_opts = mqtt::CreateOptionsBuilder::new()
        .server_uri("tcp://192.168.31.2:1883")
        .client_id("home_metrics_rs")
        .finalize();
    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .keep_alive_interval(Duration::from_secs(30))
        .clean_session(true)
        .finalize();

    let cli = mqtt::client::Client::new(create_opts).unwrap();

    // Initialize the consumer before connecting
    let rx = cli.start_consuming();
    match cli.connect(conn_opts) {
        Ok(rsp) => {
            if let Some(conn_rsp) = rsp.connect_response() {
                println!(
                    "Connected to: '{}' with MQTT version {}",
                    conn_rsp.server_uri, conn_rsp.mqtt_version
                );
                if conn_rsp.session_present {
                    println!("  w/ client session already present on broker.");
                    return
                }
            }
            // Register subscriptions on the server
            println!("Subscribing to topics with requested QoS: {:?}...", qos);

            cli.subscribe_many(&subscriptions, &qos)
                .and_then(|rsp| {
                    rsp.subscribe_many_response()
                        .ok_or(mqtt::Error::General("Bad response"))
                })
                .and_then(|vqos| {
                    println!("QoS granted: {:?}", vqos);
                    Ok(())
                })
                .unwrap_or_else(|err| {
                    println!("Error subscribing to topics: {:?}", err);
                    cli.disconnect(None).unwrap();
                    process::exit(1);
                });
        }
        Err(e) => {
            println!("Error connection to the broker: {:?}", e);
            process::exit(1);
        }
    }
    // ^C handler will stop the consumer, breaking us out of the loop, below
    // let ctrlc_cli = cli.clone();
    // ctrlc::set_handler(move || {
    //     ctrlc_cli.stop_consuming();
    // })
    //     .expect("Error setting Ctrl-C handler");

    for msg in rx.iter() {
        if let Some(msg) = msg {
            match msg.topic().split('/').collect::<Vec<&str>>().get(1) {
                Some(topic_name) => {
                    let val: f64 = msg.payload_str().parse().unwrap();
                    m.lock().unwrap().set_metric(topic_name, val);
                },
                _ => {}
            }
        }
    }

    if cli.is_connected() {
        println!("\nDisconnecting...");
        cli.unsubscribe_many(&subscriptions).unwrap();
        cli.disconnect(None).unwrap();
    }

    println!("returning from thread");
}

fn run_server(metrics: Arc<Mutex<Metrics>>) {
    let addr = "127.0.0.1:7878";
    let listener = TcpListener::bind(addr).unwrap();

    println!("running on {}", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let response = format!("HTTP/1.1 200 OK\r\n\r\n{}", metrics
                    .lock()
                    .unwrap()
                    .get_metrics()
                );

                stream.write(response.as_bytes()).unwrap();

                stream.flush().unwrap();
            }
            Err(_) => {}
        }
    }
}

fn main(){
    let mut m = Metrics::new();
    for (name, desc) in [
        ("temperature", "Temperature in room"),
        ("pressure", "Atmosphere pressure"),
        ("humidity", "Humidity in room"),
        ("smoke", "Smoke level in the air"),
        ("propane", "Propane level in the air"),
        ("methane", "Methane level in the air"),
    ] {
        m.register_metric(name, desc)
    }

    let m = Arc::new(Mutex::new(m));

    {
        let m = Arc::clone(&m);
        thread::spawn(move || {
            run_consuming(m)
        });
    }
    {
        let m = Arc::clone(&m);
        thread::spawn(move || {
            run_server(m)
        });
    }

    match Signals::new(&[
        consts::SIGINT,
        consts::SIGTERM
    ]) {
        Ok(mut signals) => signals.forever().for_each(|sig| {
            println!("signal {} received.", sig);
            process::exit(0)
        }),
        Err(e) => println!("{}", e)
    }

    println!("closing program...");
}
