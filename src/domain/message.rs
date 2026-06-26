use std::collections::BTreeMap;

use anyhow::Result;
use bytes::Bytes;
use iroh::{EndpointAddr, EndpointId, PublicKey, SecretKey, Signature};
use iroh_gossip::TopicId;
use serde::{Deserialize, Serialize};

use crate::{domain::node::Node, util::time_now};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Invited {
        topic: TopicId,
        rnum: Vec<u8>,
        addr: EndpointAddr,
        alias: String,
        services: BTreeMap<String, u32>,
        invitor: EndpointId,
    },
    AboutMe {
        addr: EndpointAddr,
        alias: String,
        services: BTreeMap<String, u32>,
        invitor: EndpointId,
    },
    Introduce {
        invited: EndpointId,
    },
    SyncRequest {
        nodes: Vec<Node>,
    },
    SyncResponse {
        nodes: Vec<Node>,
    },
    Heartbeat,
    Left,
    ClassText {
        id: String,
        from_node: EndpointId,
        text: String,
        timestamp: u64,
    },
    ClassAck {
        message_id: String,
        from_node: EndpointId,
        timestamp: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignedMessage {
    pub(crate) from: EndpointId,
    pub(crate) data: Bytes,
    signature: Signature,
    timestamp: u64,
}

impl SignedMessage {
    const MAX_AGE_SECS: u64 = 60;
    const MAX_FUTURE_SKEW_SECS: u64 = 10;

    fn signing_bytes(data: &Bytes, timestamp: u64) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct SigningPayload<'a> {
            data: &'a [u8],
            timestamp: u64,
        }

        Ok(postcard::to_stdvec(&SigningPayload {
            data: data.as_ref(),
            timestamp,
        })?)
    }

    pub fn decode(bytes: Bytes) -> Result<Self> {
        Ok(postcard::from_bytes(bytes.as_ref())?)
    }

    pub fn verify_and_decode_message(&self) -> Result<(PublicKey, Message)> {
        let key: PublicKey = self.from;
        let signing_bytes = Self::signing_bytes(&self.data, self.timestamp)?;
        key.verify(&signing_bytes, &self.signature)?;
        let message: Message = postcard::from_bytes(&self.data)?;
        Ok((key, message))
    }

    pub fn is_fresh(&self, now: u64) -> bool {
        if self.timestamp > now.saturating_add(Self::MAX_FUTURE_SKEW_SECS) {
            return false;
        }
        now.saturating_sub(self.timestamp) <= Self::MAX_AGE_SECS
    }

    pub fn sign_and_encode(secret_key: &SecretKey, message: Message) -> Result<Bytes> {
        Self::sign_and_encode_with_timestamp(secret_key, message, time_now())
    }

    fn sign_and_encode_with_timestamp(
        secret_key: &SecretKey,
        message: Message,
        timestamp: u64,
    ) -> Result<Bytes> {
        let data: Bytes = postcard::to_stdvec(&message)?.into();
        let signing_bytes = Self::signing_bytes(&data, timestamp)?;
        let signature = secret_key.sign(&signing_bytes);
        let from: PublicKey = secret_key.public();
        let signed_message = Self {
            from,
            data,
            signature,
            timestamp,
        };
        let encoded = postcard::to_stdvec(&signed_message)?;
        Ok(encoded.into())
    }

    #[cfg(test)]
    pub(crate) fn sign_and_encode_at(
        secret_key: &SecretKey,
        message: Message,
        timestamp: u64,
    ) -> Result<Bytes> {
        Self::sign_and_encode_with_timestamp(secret_key, message, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_message_roundtrip_verifies() -> Result<()> {
        let mut rng = rand::rng();
        let sk = SecretKey::generate(&mut rng);
        let pk = sk.public();

        let msg = Message::Heartbeat;
        let encoded = SignedMessage::sign_and_encode(&sk, msg.clone())?;
        let decoded = SignedMessage::decode(encoded)?;
        let (from, decoded_msg) = decoded.verify_and_decode_message()?;

        assert_eq!(from, pk);
        matches!(decoded_msg, Message::Heartbeat);
        assert!(decoded.is_fresh(time_now()));
        Ok(())
    }

    #[test]
    fn signed_message_rejects_tampered_timestamp() -> Result<()> {
        let mut rng = rand::rng();
        let sk = SecretKey::generate(&mut rng);

        let encoded = SignedMessage::sign_and_encode(&sk, Message::Heartbeat)?;
        let mut decoded = SignedMessage::decode(encoded)?;
        decoded.timestamp = decoded
            .timestamp
            .saturating_sub(SignedMessage::MAX_AGE_SECS + 1);

        assert!(decoded.verify_and_decode_message().is_err());
        Ok(())
    }
}
