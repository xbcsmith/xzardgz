use xzardgz::agent::context::ConversationContext;
use xzardgz::agent::message::Message;

#[test]
fn test_context_management() {
    let mut ctx = ConversationContext::new("You are a bot".to_string(), 100);

    ctx.add_message(Message::user("Hello"));
    assert_eq!(ctx.get_messages().len(), 1);

    ctx.add_message(Message::assistant("Hi there"));
    assert_eq!(ctx.get_messages().len(), 2);

    ctx.clear();
    assert_eq!(ctx.get_messages().len(), 0);
}

#[test]
fn test_compaction() {
    // 100 tokens max. 1 token approx 4 chars.
    // "Hello" is 5 chars ~ 1 token.
    let mut ctx = ConversationContext::new("System".to_string(), 5);

    // Add a long message
    let long_msg = "a".repeat(40); // 10 tokens
    ctx.add_message(Message::user(&long_msg));

    assert!(ctx.current_tokens() > 5);

    let compacted = ctx.compact_if_needed().unwrap();
    assert!(compacted);
    assert_eq!(ctx.get_messages().len(), 0); // Should remove the message
}
