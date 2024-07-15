use qb::QBICommunication;
use qb_derive::QBIAsync;

pub struct QBIGDriveInit {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub token: String,
    pub refresh_token: String,
}

#[derive(QBIAsync)]
#[context(QBIGDriveInit)]
pub struct QBIGDrive {
    com: QBICommunication,
    client: google_drive::Client,
}

impl QBIGDrive {
    async fn init_async(cx: QBIGDriveInit, com: QBICommunication) -> Self {
        let client = google_drive::Client::new(
            cx.client_id,
            cx.client_secret,
            cx.redirect_uri,
            cx.token,
            cx.refresh_token,
        );

        // check connection
        let _ = client.about().get().await.unwrap();

        Self { com, client }
    }

    async fn run_async(mut self) {
        println!("{:?}", self.com.rx.recv().await.unwrap());
    }
}
