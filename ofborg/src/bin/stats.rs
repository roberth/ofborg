extern crate hyper;
extern crate amqp;
extern crate ofborg;

use std::env;
use ofborg::{easyamqp, tasks, worker, config, stats};

use amqp::Basic;
use ofborg::easyamqp::TypedWrappers;
use hyper::server::{Request, Response, Server};

use std::thread;

fn main() {
    let cfg = config::load(env::args().nth(1).unwrap().as_ref());
    ofborg::setup_log();

    println!("Hello, world!");


    let mut session = easyamqp::session_from_config(&cfg.rabbitmq).unwrap();
    println!("Connected to rabbitmq");

    let events = stats::RabbitMQ::new(
        &format!("{}-{}", cfg.runner.identity.clone(), cfg.nix.system.clone()),
        session.open_channel(3).unwrap()
    );

    let metrics = stats::MetricCollector::new();

    let collector = tasks::statscollector::StatCollectorWorker::new(
        events,
        metrics.clone(),
    );

    let mut channel = session.open_channel(1).unwrap();
    channel
        .declare_exchange(easyamqp::ExchangeConfig {
            exchange: "stats".to_owned(),
            exchange_type: easyamqp::ExchangeType::Fanout,
            passive: false,
            durable: true,
            auto_delete: false,
            no_wait: false,
            internal: false,
            arguments: None,
        })
        .unwrap();

    channel
        .declare_queue(easyamqp::QueueConfig {
            queue: "stats-events".to_owned(),
            passive: false,
            durable: true,
            exclusive: false,
            auto_delete: false,
            no_wait: false,
            arguments: None,
        })
        .unwrap();

    channel
        .bind_queue(easyamqp::BindQueueConfig {
            queue: "stats-events".to_owned(),
            exchange: "stats".to_owned(),
            routing_key: None,
            no_wait: false,
            arguments: None,
        })
        .unwrap();

    channel.basic_prefetch(1).unwrap();
    channel
        .consume(
            worker::new(collector),
            easyamqp::ConsumeConfig {
                queue: "stats-events".to_owned(),
                consumer_tag: format!("{}-prometheus-stats-collector", cfg.whoami()),
                no_local: false,
                no_ack: false,
                no_wait: false,
                exclusive: false,
                arguments: None,
            },
        )
        .unwrap();


    thread::spawn(||{
        let addr = "0.0.0.0:9898";
        println!("listening addr {:?}", addr);
        Server::http(addr)
            .unwrap()
            .handle(move |_: Request, res: Response| {
                res.send(metrics.prometheus_output().as_bytes()).unwrap();
            })
            .unwrap();
    });


    channel.start_consuming();

    println!("Finished consuming?");

    channel.close(200, "Bye").unwrap();
    println!("Closed the channel");
    session.close(200, "Good Bye");
    println!("Closed the session... EOF");
}
