use http_body_util::BodyExt;
use crate::HttpResponse;

pub async fn read_file_bytes(response: &mut HttpResponse) -> Vec<u8> {
    let mut buffer = vec![];
    while let Some(next) = response.body_mut().frame().await {
        let frame = next.unwrap();
        if let Some(chunk) = frame.data_ref() {
            buffer.extend_from_slice(chunk);
        }
    }
    
    // If the file starts with a UTF-8 BOM (EF BB BF), remove it
    if buffer.starts_with(&[0xEF, 0xBB, 0xBF]) {
        buffer.drain(0..3); // This removes the first three bytes
    }
    
    buffer
}