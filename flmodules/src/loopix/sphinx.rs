use base64::{engine::general_purpose::STANDARD, Engine};
use sphinx_packet::{header::delays::Delay, route::{Destination, Node}, SphinxPacket};
use serde::{Deserialize, Serialize};
use flarch::nodeids::NodeID;
use sphinx_packet::route::{NodeAddressBytes, DestinationAddressBytes};
use x25519_dalek::PublicKey;
use sha2::{Sha256, Digest};

#[derive(Serialize, Deserialize)]
pub struct Sphinx {
    #[serde(serialize_with = "serialize_sphinx_packet", deserialize_with = "deserialize_sphinx_packet")]
    pub inner: SphinxPacket,
}

impl Default for Sphinx {
    fn default() -> Self {
        let message = Vec::<u8>::from("hello world".as_bytes());
        let surb_identifier = [0u8; 16];
        let destination = Destination {address: destination_address_from_node_id(NodeID::from(1)), identifier: surb_identifier};
        let route = vec![Node {address: node_address_from_node_id(NodeID::from(1)), pub_key: PublicKey::from([0; 32])}];
        let delays = vec![Delay::new_from_nanos(1)];
        Sphinx { inner: SphinxPacket::new(message, &route, &destination, &delays).unwrap() }
    }
}

impl std::fmt::Debug for Sphinx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let serialized = serde_json::to_string(&self).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(serialized);
        let hash_result = hasher.finalize();
        write!(f, "{:x}", hash_result)
    }
}

pub fn serialize_sphinx_packet<S>(sphinx_packet: &SphinxPacket, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bytes = sphinx_packet.to_bytes();
    let base64_encoded = STANDARD.encode(&bytes);
    serializer.serialize_str(&base64_encoded)
}

pub fn deserialize_sphinx_packet<'de, D>(deserializer: D) -> std::result::Result<SphinxPacket, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let base64_encoded = String::deserialize(deserializer)?;
    let bytes = STANDARD.decode(&base64_encoded).map_err(serde::de::Error::custom)?;
    SphinxPacket::from_bytes(&bytes).map_err(serde::de::Error::custom)
}

impl PartialEq for Sphinx {
    fn eq(&self, other: &Self) -> bool {
        self.inner.to_bytes() == other.inner.to_bytes()
    }
}

impl Clone for Sphinx {
    fn clone(&self) -> Self {
        let mut buffer = Vec::new();
        serialize_sphinx_packet(&self.inner, &mut serde_json::Serializer::new(&mut buffer)).unwrap();
        let cloned_packet = deserialize_sphinx_packet(&mut serde_json::Deserializer::from_slice(&buffer)).unwrap();
        Sphinx { inner: cloned_packet }
    }
}

pub fn node_address_from_node_id(node_id: NodeID) -> NodeAddressBytes {
    let node_id_bytes = node_id.to_bytes();
    NodeAddressBytes::from_bytes(node_id_bytes)
}

pub fn node_id_from_node_address(node_address: NodeAddressBytes) -> NodeID {
    let node_address_bytes = node_address.as_bytes();
    NodeID::from(node_address_bytes)
}

pub fn node_id_from_destination_address(dest_addr: DestinationAddressBytes) -> NodeID {
    let dest_bytes = dest_addr.as_bytes();
    NodeID::from(dest_bytes)
}

pub fn destination_address_from_node_id(node_id: NodeID) -> DestinationAddressBytes {
    let node_id_bytes = node_id.to_bytes();
    DestinationAddressBytes::from_bytes(node_id_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sphinx_packet::header::SphinxHeader;
    use sphinx_packet::payload::Payload;
    use rand::RngCore;

    fn create_dummy_sphinx_packet() -> SphinxPacket {
        let mut header_bytes = [0u8; 348];
        rand::thread_rng().fill_bytes(&mut header_bytes);

        let mut payload_bytes = vec![0u8; 1024];
        rand::thread_rng().fill_bytes(&mut payload_bytes);

        SphinxPacket {
            header: SphinxHeader::from_bytes(&header_bytes).unwrap(),
            payload: Payload::from_bytes(&payload_bytes).unwrap(),
        }
    }

    #[test]
    fn test_sphinx_serialization() {
        let packet = create_dummy_sphinx_packet();
        let sphinx = Sphinx { inner: packet };
        
        let mut serialized = Vec::new();
        bincode::serialize_into(&mut serialized, &sphinx).unwrap();
        
        let deserialized: Sphinx = bincode::deserialize(&serialized).unwrap();
        
        assert_eq!(sphinx, deserialized);
    }

    #[test]
    fn test_sphinx_clone() {
        let packet = create_dummy_sphinx_packet();
        let sphinx = Sphinx { inner: packet };
        let cloned_sphinx = sphinx.clone();
        assert_eq!(sphinx, cloned_sphinx);
    }

    #[test]
    fn test_sphinx_equality() {
        let packet1 = create_dummy_sphinx_packet();
        let sphinx1 = Sphinx { inner: packet1 };
        let sphinx2 = sphinx1.clone();
        assert_eq!(sphinx1, sphinx2);
    }


}