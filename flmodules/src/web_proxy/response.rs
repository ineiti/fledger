use std::collections::HashMap;

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use tokio::sync::mpsc::Receiver;

use flarch::nodeids::NodeID;

#[derive(Debug)]
pub struct Response {
    proxy: NodeID,
    header: ResponseHeader,
    rx: Receiver<Bytes>,
}

impl Response {
    pub fn new(proxy: NodeID, header: ResponseHeader, rx: Receiver<Bytes>) -> Self {
        Self { proxy, header, rx }
    }

    pub fn proxy(&self) -> NodeID {
        self.proxy
    }

    pub fn headers(&self) -> ResponseHeader {
        self.header.clone()
    }

    pub async fn text(&mut self) -> anyhow::Result<String> {
        Ok(std::str::from_utf8(&self.bytes().await).map(|s| s.into())?)
    }

    pub async fn bytes(&mut self) -> Bytes {
        let mut b = BytesMut::new();
        while let Some(n) = self.chunk().await {
            b.extend(n);
        }
        b.into()
    }

    pub async fn chunk(&mut self) -> Option<Bytes> {
        self.rx.recv().await
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseMessage {
    Header(ResponseHeader),
    Body(#[serde_as(as = "Hex")] Bytes),
    Error(String),
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseHeader {
    pub status: ResponseStatus,
    pub headers: HashMap<String, Vec<String>>,
}

impl From<&reqwest::Response> for ResponseHeader {
    fn from(value: &reqwest::Response) -> Self {
        let mut headers = HashMap::new();
        for (k, v) in value.headers() {
            let values = headers.entry(k.to_string()).or_insert(vec![]);
            values.push(v.to_str().expect("converting header value").into());
        }
        Self {
            status: ResponseStatus {
                code: value.status().into(),
                msg: "".into(),
            },
            headers,
        }
    }
}

impl ResponseHeader {
    pub fn new(status: ResponseStatus, headers: HashMap<String, Vec<String>>) -> Self {
        Self { status, headers }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseStatus {
    pub code: u16,
    pub msg: String,
}

#[cfg(test)]
mod test {
    // use tokio::sync::mpsc::channel;

    // use super::*;

    // impl ResponseHeader {
    //     fn new_code(code: u16) -> Self {
    //         Self {
    //             status: ResponseStatus {
    //                 code,
    //                 msg: "test".into(),
    //             },
    //             headers: HashMap::new(),
    //         }
    //     }
    // }

    // #[tokio::test]
    // async fn test_new() -> anyhow::Result<()> {
    //     let (tx, rx) = channel(10);
    //     tx.send(ResponseMessage::Body(Bytes::from("123"))).await?;
    //     let err = Response::new(rx).await;
    //     assert_eq!(Some(ResponseError::NoHeader), err.err());

    //     let (tx, rx) = channel(10);
    //     tx.send(ResponseMessage::Header(ResponseHeader::new_code(200)))
    //         .await?;
    //     let err = Response::new(rx).await;
    //     assert_eq!(None, err.err());
    //     Ok(())
    // }

    // #[tokio::test]
    // async fn test_stream() -> anyhow::Result<()> {
    //     let (tx, rx) = channel(10);
    //     tx.send(ResponseMessage::Header(ResponseHeader::new_code(200)))
    //         .await?;
    //     let mut resp = Response::new(rx).await?;
    //     tx.send(ResponseMessage::Body(Bytes::from("1234"))).await?;
    //     tx.send(ResponseMessage::Body(Bytes::from("56"))).await?;
    //     tx.send(ResponseMessage::Done).await?;

    //     let text = resp.text().await?;
    //     assert_eq!("123456", text);

    //     let (tx, rx) = channel(10);
    //     tx.send(ResponseMessage::Header(ResponseHeader::new_code(200)))
    //         .await?;
    //     let mut resp = Response::new(rx).await?;
    //     tx.send(ResponseMessage::Body(Bytes::from("1234"))).await?;
    //     tx.send(ResponseMessage::Done).await?;

    //     assert_eq!(Some(Bytes::from("1234")), resp.chunk().await);
    //     assert_eq!(None, resp.chunk().await);

    //     Ok(())
    // }
}
