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
    SecurityParams,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = "https://disperser-goerli.eigenda.xyz:443";
    let mut client = DisperserClient::connect(endpoint).await?;

    // Example data to disperse
    let original_data = b"Example data".to_vec();
    let request = tonic::Request::new(DisperseBlobRequest {
        data: original_data.clone(),
        security_params: vec![SecurityParams {
            quorum_id: 0,
            adversary_threshold: 55,
            quorum_threshold: 80,
        }],
        account_id: "".to_string(), // TODO: Fill out
    });

    let response = client.disperse_blob(request).await?;
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
        status_response_option = Some(reply.clone());

        match blob_status {
            BlobStatus::Confirmed | BlobStatus::Finalized => {
                println!("Blob processing completed.");
                break;
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
        batch_header_hash: batch_header_hash,
        blob_index: blob_index, // Set appropriately based on how blobs are indexed in your system
    });

    let retrieve_response = client.retrieve_blob(retrieve_request).await?;
    let retrieved_data = retrieve_response.into_inner().data;

    // Check if the original data matches the retrieved data
    if original_data == retrieved_data {
        println!("Successfully verified the retrieved data matches the original data.");
    } else {
        println!("The retrieved data does not match the original data.");
    }

    Ok(())
}
