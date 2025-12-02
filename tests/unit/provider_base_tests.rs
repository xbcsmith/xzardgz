use xzardgz::providers::{Message, Role};

#[test]
fn test_message_creation() {
    let msg = Message::user("hello");
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "hello");

    let msg = Message::system("sys");
    assert_eq!(msg.role, Role::System);
    assert_eq!(msg.content, "sys");

    let msg = Message::assistant("hi");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content, "hi");
}
