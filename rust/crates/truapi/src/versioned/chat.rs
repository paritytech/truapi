//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostChatCreateRoomRequest { V1 => v01::HostChatCreateRoomRequest }
    pub enum HostChatCreateRoomResponse { V1 => v01::HostChatCreateRoomResponse }
    pub enum HostChatCreateRoomError { V1 => v01::HostChatCreateRoomError }
    pub enum HostChatRegisterBotRequest { V1 => v01::HostChatRegisterBotRequest }
    pub enum HostChatRegisterBotResponse { V1 => v01::HostChatRegisterBotResponse }
    pub enum HostChatRegisterBotError { V1 => v01::HostChatRegisterBotError }
    pub enum HostChatPostMessageRequest { V1 => v01::HostChatPostMessageRequest }
    pub enum HostChatPostMessageResponse { V1 => v01::HostChatPostMessageResponse }
    pub enum HostChatPostMessageError { V1 => v01::HostChatPostMessageError }
    pub enum HostChatListSubscribeItem { V1 => v01::HostChatListSubscribeItem }
    pub enum HostChatActionSubscribeItem { V1 => v01::HostChatActionSubscribeItem }
    pub enum ProductChatCustomMessageRenderRequest { V1 => v01::ProductChatCustomMessageRenderRequest }
    pub enum ProductChatCustomMessageRenderItem { V1 => v01::CustomRendererNode }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;

    #[test]
    fn custom_render_start_matches_legacy_wire_fixture() {
        let request =
            ProductChatCustomMessageRenderRequest::V1(v01::ProductChatCustomMessageRenderRequest {
                message_id: "message-1".into(),
                message_type: "vote".into(),
                payload: vec![1, 2],
            });

        assert_eq!(
            hex::encode(request.encode()),
            "00246d6573736167652d3110766f7465080102"
        );
    }

    #[test]
    fn custom_render_receive_matches_legacy_wire_fixture() {
        let item = ProductChatCustomMessageRenderItem::V1(v01::CustomRendererNode::String {
            text: "Votes: 1".into(),
        });

        assert_eq!(hex::encode(item.encode()), "000120566f7465733a2031");
    }
}
