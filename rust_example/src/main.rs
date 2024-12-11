mod disperser {
    tonic::include_proto!("disperser");
}

mod common {
    tonic::include_proto!("common");
}

use std::time::Duration;

use disperser::disperser_client::DisperserClient;
use disperser::{
    BlobStatus, BlobStatusReply, BlobStatusRequest, DisperseBlobRequest, RetrieveBlobRequest,
};

/// Default value for the EigenDA disperser URL
/// This is the URL for the preprod EigenDA disperser for the holesky Ethereum testnet
const DEFAULT_DISPERSER_URL: &str = "https://disperser-preprod-holesky.eigenda.xyz:443";

/// Default message to send in the blob
const DEFAULT_MESSAGE: &str = "hello world";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let endpoint = args.nth(1).unwrap_or(DEFAULT_DISPERSER_URL.to_string());
    let mut client = DisperserClient::connect(endpoint.clone()).await.unwrap();

    let message = args.next().unwrap_or(DEFAULT_MESSAGE.to_string());
    if message.len() > 31 {
        return Err("Message is too long, must be 31 characters or less.".into());
    }

    println!("Sending blob containing '{message}' to '{endpoint}'");

    // Left-pad to ensure it's valid
    let original_data: Vec<u8> = [b"\0".as_slice(), message.as_bytes()].concat();

    let request = tonic::Request::new(DisperseBlobRequest {
        data: original_data.clone(),
        custom_quorum_numbers: vec![],
        account_id: "".to_string(), // TODO: Fill out
    });

    let response = client.disperse_blob(request).await.unwrap();
    let request_id = response.into_inner().request_id;
    println!(
        "Blob dispersion completed, request id '{}'",
        base64::encode(&request_id)
    );

    // Poll GetBlobStatus with a timeout of 5 minutes
    let start_time = tokio::time::Instant::now();
    let timeout_duration = Duration::from_secs(5 * 60); // 5 minutes
    let mut blob_status = BlobStatus::Unknown;
    let mut status_response_option: Option<BlobStatusReply> = None;

    while tokio::time::Instant::now().duration_since(start_time) < timeout_duration {
        let status_request = tonic::Request::new(BlobStatusRequest {
            request_id: request_id.clone(),
        });

        println!("Checking for blob confirmation...");
        let reply = client.get_blob_status(status_request).await?.into_inner();
        blob_status = reply.status();
        status_response_option = Some(reply);

        match blob_status {
            BlobStatus::Confirmed | BlobStatus::Finalized => {
                println!("Blob processing completed.");
                break;
            }
            BlobStatus::Failed => {
                return Err("Blob processing failed.".into());
            }
            _ => {
                println!("Blob not yet confirmed, sleeping for 5 seconds.");
                tokio::time::sleep(Duration::from_secs(5)).await
            }
        }
    }

    if blob_status != BlobStatus::Confirmed && blob_status != BlobStatus::Finalized {
        return Err("Timeout reached without confirmation or finalization of the blob.".into());
    }

    let status_response = status_response_option.expect("BlobStatusReply not set");

    let info = status_response.info.as_ref().ok_or("info is None")?;
    let proof = info
        .blob_verification_proof
        .as_ref()
        .ok_or("blob_verification_proof is None")?;
    let metadata = proof
        .batch_metadata
        .as_ref()
        .ok_or("batch_metadata is None")?;
    let batch_header_hash = metadata.batch_header_hash.clone();
    let blob_index = proof.blob_index;

    // Retrieve the blob after successful dispersal
    let retrieve_request = tonic::Request::new(RetrieveBlobRequest {
        // Populate with appropriate fields
        batch_header_hash,
        blob_index, // Set appropriately based on how blobs are indexed in your system
    });

    let retrieve_response = client.retrieve_blob(retrieve_request).await?;
    let retrieved_data = retrieve_response.into_inner().data;

    println!(
        "Retrieved blob with data '{}'",
        String::from_utf8(retrieved_data[1..].to_vec()).unwrap()
    );

    // Check if the original data matches the retrieved data
    if original_data == retrieved_data {
        println!("Successfully verified the retrieved data matches the original data.");
    } else {
        println!("The retrieved data does not match the original data.");
    }

    Ok(())
}
