#[derive(Debug, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub commit: Commit,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub commit: CommitDetails,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitDetails {
    pub author: Signature,
    pub committer: Signature,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
}

async fn get_example() -> Result<(), JsValue>{
    let mut opts = RequestInit::new();
    // opts.method("GET");
    opts.method("POST");
    opts.mode(RequestMode::Cors);

    let url = format!("https://api.github.com/repos/{}/branches/master", "ineiti/cothority");

    let request = Request::new_with_str_and_init(&url, &opts)?;

    request
        .headers()
        .set("Content-Type", "application/json")?;
    request
        .headers()
        .set("Accept", "application/vnd.github.v3+json")?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    // `resp_value` is a `Response` object.
    assert!(resp_value.is_instance_of::<Response>());
    let resp: Response = resp_value.dyn_into().unwrap();

    // Convert this other `Promise` into a rust `Future`.
    let json = JsFuture::from(resp.json()?).await?;

    // Use serde to parse the JSON into a struct.
    let branch_info: Branch = json.into_serde().unwrap();

    // Send the `Branch` struct back to JS as an `Object`.
    console_log!("{:?}", JsValue::from_serde(&branch_info).unwrap());

    Ok(())
}
