#[tokio::main]
async fn main() {
    let t1 = tmpdir::TmpDir::new("ledger").await.unwrap();
    let t2 = tmpdir::TmpDir::new("ledger").await.unwrap();
    println!("{:?}", t1.to_path_buf());
    println!("{:?}", t2.to_path_buf());
}
