use tokio::time::Instant;

#[derive(Clone)]
pub struct InfluxApi {
    client: reqwest::blocking::Client,
}

impl InfluxApi {
    pub fn new() -> Self {
        InfluxApi {
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn write(&self, influx_data: String) -> Result<String, reqwest::Error> {
        let start = Instant::now();
        match self.
            client
            .post("https://influxdb.abehssera.com/api/v2/write?org=fledger&bucket=fledger&precision=ns")
            .body(influx_data)
            .header(
                "Authorization",
                "Token F7y_RJHnXA0szQHDhEiuRDAw7B2etGywSc-wdMK-BJtkXwplqXe5ogCcXDEJJR18ZvWJ87kwxckl6n1lFu9B-Q==",
            )
            .send()
        {
            Ok(resp) => {
                let text  = resp.text()?;
                log::info!(
                    "Successful write to influxdb ({}ms): {}",
                    start.elapsed().as_millis(),
                    text.clone(),
                );
                Ok(text)
            },
            Err(err) => {
                log::error!("Error when writing to influxdb: {}", err);
                Err(err.into())
            },
        }
    }
}
