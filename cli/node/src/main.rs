use clap::Clap;
use headless_chrome::Browser;
use serde_json::Value;
use std::{fs, thread, time::{self, Duration}};

#[derive(Clap)]
#[clap(version = "0.1", author = "Linus Gasser <linus@gasser.blue>")]
struct Opts {
    /// Sets the config file name. If it doesn't exist, it will be created with
    /// random values.
    #[clap(short, long, default_value = "fledger.toml")]
    config: String,

    /// Which server to contact. By default, contact https://web.fledg.re
    #[clap(short, long, default_value = "https://web.fledg.re")]
    server: String,
}

fn start_node() -> Result<(), failure::Error> {
    let opts: Opts = Opts::parse();

    let browser = Browser::default()?;

    let tab = browser.wait_for_initial_tab()?;

    let config = match fs::read_to_string(opts.config.clone()) {
        Ok(s) => s,
        Err(_) => "".to_string(),
    };
    println!("Using config: {}", config);

    // Navigate to the local page
    println!("Fetching code from: {}", opts.server);
    tab.navigate_to(&format!(
        "{}/#{}",
        opts.server,
        urlencoding::encode(&config)
    ))?;

    // Wait for the log to come.
    let log = tab.wait_for_element_with_custom_timeout("pre#log", Duration::from_millis(10000))?;

    // Wait for "Starting node" to appear in the log
    for i in 0..10 {
        if let Some(_) = log.get_inner_text()?.find("Starting node") {
            break;
        }
        println!("Waiting - {}", i);
        thread::sleep(time::Duration::from_millis(1000));
    }

    let local_storage = log.call_js_fn(
        "function(){return localStorage.getItem('nodeConfig')}",
        false,
    )?;
    if let Value::String(config) = local_storage.value.unwrap() {
        println!("Writing config to {} => {:?}", opts.config, config);
        fs::write(opts.config, config)?;
    }

    let mut known_log = "".to_string();
    loop {
        let new_log = log.get_inner_text()?.replace("log:\n", "").replace(&known_log, "");
        // println!("known: {}\nnew_log: {}", known_log, new_log);

        let new_log_lines: Vec<&str> = new_log.split('\n').rev().collect();
        print!("{}", new_log_lines.join("\n"));
        known_log = known_log.to_string() + &new_log;
        thread::sleep(time::Duration::from_millis(5000));
    }
}

fn main() {
    // println!("version: {}", env!("VERGEN_BUILD_TIMESTAMP"));
    if let Err(e) = start_node(){
        println!("Got error: {}", e);
    }
}
