#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    nostorgraph::launch().await
}
