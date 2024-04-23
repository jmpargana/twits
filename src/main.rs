use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use sqlite::Connection;
use std::sync::{Arc, Mutex};
use warp::Filter;

#[derive(Serialize, Deserialize, Debug)]
struct Twit {
    id: Option<i64>,
    user: String,
    content: String,
    created_at: Option<i64>,
}

struct TwitterServer {
    db: Arc<Mutex<Connection>>,
    hbs: Arc<Handlebars<'static>>,
}

mod filters {
    use crate::Twit;

    use super::handlers;
    use handlebars::Handlebars;
    use serde::Deserialize;
    use sqlite::Connection;
    use std::sync::{Arc, Mutex};
    use warp::Filter;

    pub fn twits(
        db: Arc<Mutex<Connection>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        list_twits(db.clone()).or(create_twit(db.clone()))
    }

    pub fn list_twits(
        db: Arc<Mutex<Connection>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path!("twits")
            .and(warp::get())
            .and(warp::any().map(move || db.clone()))
            .and_then(handlers::list_twits)
    }

    pub fn create_twit(
        db: Arc<Mutex<Connection>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        #[derive(Deserialize)]
        struct PartialTwit {
            content: String,
        }
        warp::path!("twits")
            .and(warp::post())
            .and(warp::addr::remote())
            .and(warp::body::form::<PartialTwit>())
            .map(|ip: Option<std::net::SocketAddr>, pt: PartialTwit| Twit {
                id: None,
                user: ip.unwrap().ip().to_string(),
                content: pt.content,
                created_at: None,
            })
            .and(warp::any().map(move || db.clone()))
            .and_then(handlers::create_twit)
    }

    pub fn html(
        db: Arc<Mutex<Connection>>,
        hbs: Arc<Handlebars<'static>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        twits_html(db.clone(), hbs.clone()).or(index_html(hbs.clone()))
    }

    pub fn index_html(
        hbs: Arc<Handlebars<'static>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::get().and_then(move || handlers::index_html(hbs.clone()))
    }

    pub fn twits_html(
        db: Arc<Mutex<Connection>>,
        hbs: Arc<Handlebars<'static>>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path!("twits_html")
            .and(warp::get())
            .and_then(move || handlers::list_twit_html(db.clone(), hbs.clone()))
    }
}

mod handlers {
    use handlebars::Handlebars;
    use serde::Serialize;
    use sqlite::{Connection, State};
    use std::{
        convert::Infallible,
        sync::{Arc, Mutex},
    };

    use crate::Twit;

    pub async fn list_twits_from_db(db: Arc<Mutex<Connection>>) -> Vec<Twit> {
        let conn = db.lock().unwrap();
        let query = "SELECT * FROM twits";
        let mut stmt = conn.prepare(query).unwrap();

        let mut twits = Vec::new();
        while let Ok(State::Row) = stmt.next() {
            let t = Twit {
                id: Some(stmt.read::<i64, _>("id").unwrap()),
                user: stmt.read::<String, _>("user").unwrap(),
                content: stmt.read::<String, _>("content").unwrap(),
                created_at: Some(stmt.read::<i64, _>("createdAt").unwrap()),
            };
            twits.push(t);
        }

        twits
    }

    pub async fn list_twits(db: Arc<Mutex<Connection>>) -> Result<impl warp::Reply, Infallible> {
        let twits = list_twits_from_db(db).await;
        Ok(warp::reply::json(&twits))
    }

    pub async fn list_twit_html(
        db: Arc<Mutex<Connection>>,
        hbs: Arc<Handlebars<'static>>,
    ) -> Result<impl warp::Reply, Infallible> {
        let twits = list_twits_from_db(db).await;

        #[derive(Serialize)]
        struct TwitList {
            twits: Vec<Twit>,
        }
        let twit_obj = TwitList { twits };

        let rendered = hbs.render("twits_html", &twit_obj).unwrap();

        Ok(warp::reply::html(rendered))
    }

    pub async fn create_twit(
        twit: Twit,
        db: Arc<Mutex<Connection>>,
    ) -> Result<impl warp::Reply, Infallible> {
        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("INSERT INTO twits (user, content) VALUES (?,?);")
            .unwrap();
        let _ = stmt.bind((1, twit.user.as_str()));
        let _ = stmt.bind((2, twit.content.as_str())).unwrap();
        let result = stmt.next().unwrap();
        println!("{:?}", result);
        Ok(warp::reply::json(&twit))
    }

    pub async fn index_html(hbs: Arc<Handlebars<'static>>) -> Result<impl warp::Reply, Infallible> {
        #[derive(Serialize)]
        struct IndexTemplate {
            title: String,
        }
        let title = IndexTemplate {
            title: "Test".to_string(),
        };
        let rendered = hbs.render("index", &title).unwrap();
        Ok(warp::reply::html(rendered))
    }
}

impl TwitterServer {
    fn new() -> TwitterServer {
        // DB Setup
        let conn = sqlite::open(":memory:").unwrap();
        let twit_table = "
    CREATE TABLE twits (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user TEXT NOT NULL, 
        content TEXT NOT NULl, 
        createdAt TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    );

    INSERT INTO twits (user, content) VALUES ('Bob', 'First twit');
    ";
        conn.execute(twit_table).unwrap();

        // Template Setup
        let mut hbs = Handlebars::new();

        let _ = hbs
            .register_template_file("index", "./src/templates/index.hbs")
            .expect("Template should have been registered properly");
        hbs.register_template_file("twits_html", "./src/templates/twits.hbs")
            .unwrap();
        let hbs = Arc::new(hbs);

        TwitterServer {
            db: Arc::new(Mutex::new(conn)),
            hbs,
        }
    }
}

#[tokio::main]
async fn main() {
    let srv = TwitterServer::new();
    let routes = filters::twits(srv.db.clone()).or(filters::html(srv.db.clone(), srv.hbs));
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
