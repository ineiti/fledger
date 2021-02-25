use headless_chrome::Browser;
use std::{env, fs, thread, time};
use serde_json::Value;

fn start_node() -> Result<(), failure::Error> {
    let args: Vec<String> = env::args().collect();
    let config_name = if args.len() < 2 {
        "fledger.toml"
    } else {
        &args[1]
    };

    let browser = Browser::default()?;

    let tab = browser.wait_for_initial_tab()?;

    let config = match fs::read_to_string(config_name) {
        Ok(s) => s,
        Err(_) => "".to_string(),
    };
    println!("Using config: {}", config);

    // Navigate to the local page
    tab.navigate_to(&format!(
        "http://localhost:8080/?config={}",
        urlencoding::encode(&config)
    ))?;

    // Wait for the log to come.
    let log = tab.wait_for_element("pre#log")?;

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
        println!("Config: {} / {:?}", config_name, config);
        fs::write(config_name, config)?;
    }

    loop {
        println!("{}", log.get_inner_text()?);
        thread::sleep(time::Duration::from_millis(5000));
    }
}

fn main() {
    assert!(start_node().is_ok());
}
