use std::io;
use std::io::Write;
use std::process::Command;
use url::form_urlencoded;
use webbrowser;

fn get_single_number(stdin: &io::Stdin) -> io::Result<u32> {
	let mut total_str = String::new();
	stdin.read_line(&mut total_str)?;
	if total_str == "\n" {
		return Ok(1);
	}
	total_str
		.trim()
		.parse::<u32>()
		.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

fn get_number(stdin: &io::Stdin) -> u32 {
	std::iter::repeat_with(|| get_single_number(stdin))
		.skip_while(|t| t.is_err())
		.next()
		.unwrap()
		.unwrap()
}

fn get_number_in_range(stdin: &io::Stdin, stdout: &mut io::Stdout, min: u32, max: u32) -> u32 {
	std::iter::repeat_with(|| {
		print!("Type your answer[1-3] (default 1): ");
		stdout.flush().expect("Couldn't flush stdout!");
		get_number(stdin)
	})
	.skip_while(|&t| t < min || t > max)
	.next()
	.unwrap()
}

fn get_cpuinfo() -> io::Result<String> {
	Command::new("cat")
		.arg("/proc/cpuinfo")
		.output()
		.and_then(|o| {
			String::from_utf8(o.stdout).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
		})
}

fn get_lspci() -> io::Result<String> {
	Command::new("lspci").output().and_then(|o| {
		String::from_utf8(o.stdout).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
	})
}

fn main() -> io::Result<()> {
	let stdin = io::stdin();
	let mut stdout = io::stdout();
	let mut title = String::new();

	println!("Welcome to the bug report tool. This will open a GitLab issue window after collecting some information");
	print!("Please, provide a title for your bug report: ");
	stdout.flush()?;
	stdin.read_line(&mut title)?;
	title.pop();

	println!("Do you know what kind of bug this is? If not, just leave this empty");
	println!("1 - Unsure/Bug in epic node");
	println!("2 - Bug in epic miner");
	println!("3 - Bug in epic wallet");
	stdout.flush()?;
	let repo = match get_number_in_range(&stdin, &mut stdout, 1, 3) {
		1 => "epic",
		2 => "epic-miner",
		3 => "epicwallet",
		_ => panic!("Somehow you got an invalid number in this step. I don't know how you did it, but contact us at developers@brickabode.com to let us know."),
	};
	let cpuinfo = get_cpuinfo()?;
	let pciinfo = get_lspci()?;

	let encoded: String = form_urlencoded::Serializer::new(String::new())
		.append_pair("issue[title]", &title)
		.append_pair(
			"issue[description]",
			&format!(
				"INSERT THE DESCRIPTION OF YOUR BUG HERE\n\n---------------\n{}\n\n{}",
				cpuinfo, pciinfo
			),
		)
		.finish();

	webbrowser::open(&format!(
		"gitlab.com/epiccash/{}/issues/new?{}",
		repo, encoded
	))?;

	Ok(())
}
