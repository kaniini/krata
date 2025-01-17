use std::{env, process};
use xenclient::error::Result;
use xenclient::{DomainConfig, XenClient};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("usage: boot <kernel-image> <initrd>");
        process::exit(1);
    }
    let kernel_image_path = args.get(1).expect("argument not specified");
    let initrd_path = args.get(2).expect("argument not specified");
    let client = XenClient::open(0).await?;
    let config = DomainConfig {
        backend_domid: 0,
        name: "xenclient-test",
        max_vcpus: 1,
        mem_mb: 512,
        kernel_path: kernel_image_path.as_str(),
        initrd_path: initrd_path.as_str(),
        cmdline: "debug elevator=noop",
        use_console_backend: None,
        disks: vec![],
        channels: vec![],
        vifs: vec![],
        filesystems: vec![],
        extra_keys: vec![],
        extra_rw_paths: vec![],
        event_channels: vec![],
    };
    let created = client.create(&config).await?;
    println!("created domain {}", created.domid);
    Ok(())
}
