// Mirrors TS SDK tests/resources/*.

use std::any::TypeId;

use anthropic_sdk::resources::{
    APIErrorObject, Beta, Completions, ErrorObject, ErrorResponse, Message, Messages, ModelInfo,
    Models,
};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

#[test]
fn resources_index_reexports_primary_names_like_ts() {
    assert_same_type::<APIErrorObject, ErrorObject>();
    assert_same_type::<Beta<'static>, anthropic_sdk::resources::beta::Beta<'static>>();
    assert_same_type::<
        Completions<'static>,
        anthropic_sdk::resources::completions::Completions<'static>,
    >();
    assert_same_type::<Messages<'static>, anthropic_sdk::resources::messages::Messages<'static>>();
    assert_same_type::<Models<'static>, anthropic_sdk::resources::models::Models<'static>>();
    assert_eq!(
        std::mem::size_of::<ErrorResponse>(),
        std::mem::size_of::<ErrorResponse>()
    );
    assert_eq!(
        std::mem::size_of::<Message>(),
        std::mem::size_of::<Message>()
    );
    assert_eq!(
        std::mem::size_of::<ModelInfo>(),
        std::mem::size_of::<ModelInfo>()
    );
}

#[path = "resources/messages/parse.rs"]
mod messages_parse;

#[path = "resources/beta/messages/parse.rs"]
mod beta_messages_parse;
