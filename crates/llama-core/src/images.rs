//! Define APIs for image generation and edit.

use crate::{error::LlamaCoreError, SD_IMAGE_TO_IMAGE, SD_TEXT_TO_IMAGE};
use base64::{engine::general_purpose, Engine as _};
use endpoints::images::{
    ImageCreateRequest, ImageEditRequest, ImageObject, ImageVariationRequest, ListImagesResponse,
    ResponseFormat, SamplingMethod,
};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
};
use wasmedge_stable_diffusion::{stable_diffusion_interface::ImageType, BaseFunction};

/// Create an image given a prompt.
pub async fn image_generation(
    req: &mut ImageCreateRequest,
) -> Result<ListImagesResponse, LlamaCoreError> {
    #[cfg(feature = "logging")]
    info!(target: "stdout", "Processing the image generation request.");

    let text_to_image_ctx = match SD_TEXT_TO_IMAGE.get() {
        Some(sd) => sd,
        None => {
            let err_msg = "Fail to get the underlying value of `SD_TEXT_TO_IMAGE`.";

            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            return Err(LlamaCoreError::Operation(err_msg.into()));
        }
    };

    let mut context = text_to_image_ctx.lock().map_err(|e| {
        let err_msg = format!("Fail to acquire the lock of `SD_TEXT_TO_IMAGE`. {}", e);

        #[cfg(feature = "logging")]
        error!(target: "stdout", "{}", &err_msg);

        LlamaCoreError::Operation(err_msg)
    })?;

    #[cfg(feature = "logging")]
    info!(target: "stdout", "sd text_to_image context: {:?}", &context);

    let ctx = &mut *context;

    // create a unique file id
    let id = format!("file_{}", uuid::Uuid::new_v4());

    // save the file
    let path = Path::new("archives");
    if !path.exists() {
        fs::create_dir(path).unwrap();
    }
    let file_path = path.join(&id);
    if !file_path.exists() {
        fs::create_dir(&file_path).unwrap();
    }
    let filename = "output.png";
    let output_image_file = file_path.join(filename);
    let output_image_file = output_image_file.to_str().unwrap();

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "prompt: {}", &req.prompt);

    // negative prompt
    let negative_prompt = req.negative_prompt.clone().unwrap_or_default();

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "negative prompt: {}", &negative_prompt);

    // cfg_scale
    let cfg_scale = req.cfg_scale.unwrap_or(7.0);

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "cfg_scale: {}", cfg_scale);

    // sampling method
    let sample_method = req.sample_method.unwrap_or(SamplingMethod::EulerA);

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "sample_method: {}", sample_method);

    // convert sample method to value of `SampleMethodT` type
    let sample_method = match sample_method {
        SamplingMethod::Euler => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::EULER
        }
        SamplingMethod::EulerA => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::EULERA
        }
        SamplingMethod::Heun => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::HEUN
        }
        SamplingMethod::Dpm2 => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::DPM2
        }
        SamplingMethod::DpmPlusPlus2sA => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::DPMPP2SA
        }
        SamplingMethod::DpmPlusPlus2m => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::DPMPP2M
        }
        SamplingMethod::DpmPlusPlus2mv2 => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::DPMPP2Mv2
        }
        SamplingMethod::Ipndm => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::IPNDM
        }
        SamplingMethod::IpndmV => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::IPNDMV
        }
        SamplingMethod::Lcm => {
            wasmedge_stable_diffusion::stable_diffusion_interface::SampleMethodT::LCM
        }
    };

    // steps
    let steps = req.steps.unwrap_or(20);

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "steps: {}", steps);

    // size
    let height = req.height.unwrap_or(512);
    let width = req.width.unwrap_or(512);

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "height: {}, width: {}", height, width);

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "generate image");

    ctx.set_prompt(&req.prompt)
        .set_negative_prompt(negative_prompt)
        .set_output_path(output_image_file)
        .set_cfg_scale(cfg_scale)
        .set_sample_method(sample_method)
        .set_sample_steps(steps as i32)
        .set_height(height as i32)
        .set_width(width as i32)
        .generate()
        .map_err(|e| {
            let err_msg = format!("Fail to dump the image. {}", e);

            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            LlamaCoreError::Operation(err_msg)
        })?;

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "file_id: {}, file_name: {}", &id, &filename);

    let image = match req.response_format {
        Some(ResponseFormat::B64Json) => {
            // convert the image to base64 string
            let base64_string = match image_to_base64(output_image_file) {
                Ok(base64_string) => base64_string,
                Err(e) => {
                    let err_msg = format!("Fail to convert the image to base64 string. {}", e);

                    #[cfg(feature = "logging")]
                    error!(target: "stdout", "{}", &err_msg);

                    return Err(LlamaCoreError::Operation(err_msg));
                }
            };

            // log
            #[cfg(feature = "logging")]
            info!(target: "stdout", "base64 string: {}", &base64_string.chars().take(10).collect::<String>());

            // create an image object
            ImageObject {
                b64_json: Some(base64_string),
                url: None,
                prompt: Some(req.prompt.clone()),
            }
        }
        Some(ResponseFormat::Url) | None => {
            // create an image object
            ImageObject {
                b64_json: None,
                url: Some(format!("/archives/{}/{}", &id, &filename)),
                prompt: Some(req.prompt.clone()),
            }
        }
    };

    let created: u64 = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => {
            let err_msg = "Failed to get the current time.";

            // log
            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            return Err(LlamaCoreError::Operation(err_msg.into()));
        }
    };

    let res = ListImagesResponse {
        created,
        data: vec![image],
    };

    #[cfg(feature = "logging")]
    info!(target: "stdout", "End of the image generation.");

    Ok(res)
}

/// Create an edited or extended image given an original image and a prompt.
pub async fn image_edit(req: &mut ImageEditRequest) -> Result<ListImagesResponse, LlamaCoreError> {
    let image_to_image_ctx = match SD_IMAGE_TO_IMAGE.get() {
        Some(sd) => sd,
        None => {
            let err_msg = "Fail to get the underlying value of `SD_IMAGE_TO_IMAGE`.";

            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            return Err(LlamaCoreError::Operation(err_msg.into()));
        }
    };

    let mut context = image_to_image_ctx.lock().map_err(|e| {
        let err_msg = format!("Fail to acquire the lock of `SD_IMAGE_TO_IMAGE`. {}", e);

        #[cfg(feature = "logging")]
        error!(target: "stdout", "{}", &err_msg);

        LlamaCoreError::Operation(err_msg)
    })?;

    #[cfg(feature = "logging")]
    info!(target: "stdout", "sd image_to_image context: {:?}", &context);

    let ctx = &mut *context;

    // create a unique file id
    let id = format!("file_{}", uuid::Uuid::new_v4());

    // save the file
    let path = Path::new("archives");
    if !path.exists() {
        fs::create_dir(path).unwrap();
    }
    let file_path = path.join(&id);
    if !file_path.exists() {
        fs::create_dir(&file_path).unwrap();
    }
    let filename = "output.png";
    let output_image_file = file_path.join(filename);
    let output_image_file = output_image_file.to_str().unwrap();

    // get the path of the original image
    let origin_image_file = Path::new("archives")
        .join(&req.image.id)
        .join(&req.image.filename);
    let path_origin_image = origin_image_file.to_str().ok_or(LlamaCoreError::Operation(
        "Fail to get the path of the original image.".into(),
    ))?;

    // create and dump the generated image
    ctx.set_prompt(&req.prompt)
        .set_image(ImageType::Path(path_origin_image.into()))
        .set_output_path(output_image_file)
        .generate()
        .map_err(|e| {
            let err_msg = format!("Fail to dump the image. {}", e);

            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            LlamaCoreError::Operation(err_msg)
        })?;

    // log
    #[cfg(feature = "logging")]
    info!(target: "stdout", "file_id: {}, file_name: {}", &id, &filename);

    let image = match req.response_format {
        Some(ResponseFormat::B64Json) => {
            // convert the image to base64 string
            let base64_string = match image_to_base64(output_image_file) {
                Ok(base64_string) => base64_string,
                Err(e) => {
                    let err_msg = format!("Fail to convert the image to base64 string. {}", e);

                    #[cfg(feature = "logging")]
                    error!(target: "stdout", "{}", &err_msg);

                    return Err(LlamaCoreError::Operation(err_msg));
                }
            };

            // log
            #[cfg(feature = "logging")]
            info!(target: "stdout", "base64 string: {}", &base64_string.chars().take(10).collect::<String>());

            // create an image object
            ImageObject {
                b64_json: Some(base64_string),
                url: None,
                prompt: Some(req.prompt.clone()),
            }
        }
        Some(ResponseFormat::Url) | None => {
            // create an image object
            ImageObject {
                b64_json: None,
                url: Some(format!("/archives/{}/{}", &id, &filename)),
                prompt: Some(req.prompt.clone()),
            }
        }
    };

    let created: u64 = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => {
            let err_msg = "Failed to get the current time.";

            // log
            #[cfg(feature = "logging")]
            error!(target: "stdout", "{}", &err_msg);

            return Err(LlamaCoreError::Operation(err_msg.into()));
        }
    };

    Ok(ListImagesResponse {
        created,
        data: vec![image],
    })
}

/// Create a variation of a given image.
pub async fn image_variation(
    _req: &mut ImageVariationRequest,
) -> Result<ListImagesResponse, LlamaCoreError> {
    unimplemented!("image_variation")
}

// convert an image file to a base64 string
fn image_to_base64(image_path: &str) -> io::Result<String> {
    // Open the file
    let mut image_file = File::open(image_path)?;

    // Read the file into a byte array
    let mut buffer = Vec::new();
    image_file.read_to_end(&mut buffer)?;

    Ok(general_purpose::STANDARD.encode(&buffer))
}