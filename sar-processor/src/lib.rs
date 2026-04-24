wit_bindgen::generate!({
    path: "../wit",
    world: "sar-processor",
    generate_all,
});

use crate::wasmcloud::messaging::types::BrokerMessage;
use wasmcloud::messaging::consumer;
#[allow(unused)]
use wstd::prelude::*;

struct Component;
export!(Component);

impl exports::wasmcloud::messaging::handler::Guest for Component {
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        let Some(subject) = msg.reply_to else {
            return Err("missing reply_to".to_string());
        };

        let reply = BrokerMessage {
            subject,
            body: b"sar-processor ready".to_vec(),
            reply_to: None,
        };

        consumer::publish(&reply)
    }
}
