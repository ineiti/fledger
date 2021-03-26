use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name="Signal server", about="Listens for connections of fledger-nodes to set up webrtc connections")]
pub struct Config {
    // msec between two cleanup procedures where nodes without a ping are removed.
    #[structopt(short, long="cleanup", default_value="30")]
    pub cleanup_interval: u64,
    // graphite endpoint to use
    #[structopt(short="g", long="graphite-host")]
    pub graphite_host_port: Option<String>,
    // path for the series - additional tags will be used for the statistics
    #[structopt(short="a", long="graphite-path")]
    pub graphite_path: Option<String>,
    // debug detail - 0: only warnings and errors .. 5: everything possible
    #[structopt(short, long="debug", default_value="2")]
    pub debug_lvl: u32,
    // port where the server will listen
    #[structopt(short, long, default_value="8765")]
    pub port: u32,
    // csv file for stats
    #[structopt(long)]
    pub file_stats: Option<String>,
    // csv file for node infos
    #[structopt(long)]
    pub file_nodes: Option<String>,
}

impl Config {
    pub fn logger_str(&self) -> &str {
        match self.debug_lvl {
            0 => "error",
            1 => "warn",
            2 => "info",
            3 => "debug",
            4 => "trace",
            _ => panic!("Invalid debug level")
        }
    }
}