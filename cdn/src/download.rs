use reqwest;
use std::io::File;

async fn download() {
	let target = "https://epiccash.s3.sa-east-1.amazonaws.com/mainnet.zip";
	let response = reqwest::get(target).await?;

	let path = Path::new("./download.zip");

	let mut file = match File::create(&path) {
		Err(why) => panic!("couldn't create {}", why),
		Ok(file) => file,
	};
	let content = response.bytes().await?;
	file.write_all(&content)?;
}
