use rtnetlink::{new_connection, Handle};
async fn test(handle: Handle) {
    let _ = handle.link().set(1).up().execute().await;
}
