#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::fmt::Write as FmtWrite;
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::TcpClientState;
use embassy_time::{Duration, Timer, with_timeout};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Config, init, rng::Rng};
use esp_radio::wifi;
use esp32_time_bus::parser;
use heapless::String;
use log::{info, warn};
use reqwless::client::TlsConfig;
use reqwless::request::RequestBuilder;
use static_cell::StaticCell;

extern crate alloc;

static NET_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
static RADIO_CONTROLLER: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
static TCP_CLIENT_STATE: StaticCell<TcpClientState<1, 1024, 1024>> = StaticCell::new();

const SSID: &str = match option_env!("WIFI_SSID") {
    Some(value) => value,
    None => "YOUR_WIFI_SSID",
};
const PASSWORD: &str = match option_env!("WIFI_PASSWORD") {
    Some(value) => value,
    None => "YOUR_WIFI_PASSWORD",
};
const WIFI_RETRY_DELAY_SECS: u64 = 3;
const API_HOST: &str = "prim.iledefrance-mobilites.fr";
const API_BASE_PATH: &str = "/marketplace";
const API_TOKEN: &str = match option_env!("IDFM_API_KEY") {
    Some(value) => value,
    None => "YOUR_IDFM_API_KEY",
};
const MONITORING_REF_ENCODED: &str = match option_env!("MONITORING_REF_ENCODED") {
    Some(value) => value,
    None => "YOUR_MONITORING_REF_ENCODED",
};
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 30;
const HTTP_SEND_TIMEOUT_SECS: u64 = 20;
const API_REQUEST_RETRIES: u32 = 3;
const API_RETRY_DELAY_SECS: u64 = 3;
const API_RESPONSE_PREVIEW_BYTES: usize = 8192;

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.2.0

    esp_println::logger::init_logger_from_env();

    let config = Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let radio_init = RADIO_CONTROLLER
        .init(esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller"));
    let (mut wifi_controller, interfaces) =
        wifi::new(radio_init, peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    let wifi_interface = interfaces.sta;
    let config = embassy_net::Config::dhcpv4(Default::default());

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        NET_RESOURCES.init(StackResources::<3>::new()),
        seed,
    );

    spawner
        .spawn(net_task(runner))
        .expect("Failed to spawn net task");

    let client_config = wifi::ClientConfig::default()
        .with_ssid(SSID.into())
        .with_password(PASSWORD.into());

    wifi_controller
        .set_config(&wifi::ModeConfig::Client(client_config))
        .expect("Failed to set Wi-Fi client config");
    wifi_controller
        .start_async()
        .await
        .expect("Failed to start Wi-Fi controller");

    let mut connect_attempt = 0u32;
    loop {
        connect_attempt += 1;
        match wifi_controller.connect_async().await {
            Ok(()) => break,
            Err(err) => {
                warn!(
                    "Wi-Fi connect attempt {} failed: {:?}; retrying in {}s",
                    connect_attempt, err, WIFI_RETRY_DELAY_SECS
                );
                Timer::after(Duration::from_secs(WIFI_RETRY_DELAY_SECS)).await;
            }
        }
    }

    info!("Wi-Fi connected");

    info!("Waiting for Wi-Fi link up...");
    stack.wait_link_up().await;

    info!("Waiting for DHCP lease...");
    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        info!("ESP IP address: {}", cfg.address.address());
        info!("ESP CIDR: {}", cfg.address);
    }

    let dns = DnsSocket::new(stack);
    let tcp_state = TCP_CLIENT_STATE.init(TcpClientState::new());
    let tcp_client = embassy_net::tcp::client::TcpClient::new(stack, tcp_state);

    // Configure TLS for HTTPS using embedded-tls
    let tls_seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let mut tls_read_buf = [0u8; 4096];
    let mut tls_write_buf = [0u8; 4096];
    let tls_config = TlsConfig::new(
        tls_seed,
        &mut tls_read_buf,
        &mut tls_write_buf,
        reqwless::client::TlsVerify::None,
    );
    let mut http_client = reqwless::client::HttpClient::new_with_tls(&tcp_client, &dns, tls_config);
    let mut rx_buf = [0u8; 4096];

    // Build full API URL
    let mut url_buf: String<256> = String::new();
    let _ = write!(
        url_buf,
        "https://{}{}/stop-monitoring?MonitoringRef={}",
        API_HOST, API_BASE_PATH, MONITORING_REF_ENCODED
    );

    info!("API request: GET {}", url_buf.as_str());
    let mut attempt = 0u32;
    while attempt < API_REQUEST_RETRIES {
        attempt += 1;
        info!("API attempt {}/{}", attempt, API_REQUEST_RETRIES);
        match with_timeout(
            Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS),
            http_client.request(reqwless::request::Method::GET, url_buf.as_str()),
        )
        .await
        {
            Ok(Ok(mut req)) => {
                info!("API request created, adding headers...");
                req = req.headers(&[("accept", "application/json"), ("apiKey", API_TOKEN)]);
                info!("Headers added, sending request...");
                match with_timeout(
                    Duration::from_secs(HTTP_SEND_TIMEOUT_SECS),
                    req.send(&mut rx_buf),
                )
                .await
                {
                    Ok(Ok(resp)) => {
                        info!("API GET status: {}", resp.status.0);

                        let mut reader = resp.body().reader();
                        let mut preview = [0u8; API_RESPONSE_PREVIEW_BYTES];
                        match reader.read_to_end(&mut preview).await {
                            Ok(total_len) => {
                                info!("API response bytes read: {}", total_len);
                                if total_len > 0 {
                                    if let Ok(body_str) =
                                        core::str::from_utf8(&preview[..total_len])
                                    {
                                        let departures =
                                            parser::extract_expected_departures(body_str);
                                        let current_ts =
                                            parser::extract_response_timestamp(body_str);

                                        if let Some(now) = current_ts.as_ref() {
                                            if let Some(hhmm) = parser::to_hhmm(now.as_str()) {
                                                info!(
                                                    "Current API time: {} ({})",
                                                    hhmm,
                                                    now.as_str()
                                                );
                                            } else {
                                                info!("Current API timestamp: {}", now.as_str());
                                            }
                                        } else {
                                            warn!("ResponseTimestamp not found in response");
                                        }

                                        if departures.is_empty() {
                                            warn!("No ExpectedDepartureTime found in response");
                                        } else {
                                            info!("Next departures found: {}", departures.len());
                                            for (idx, dep) in departures.iter().enumerate() {
                                                if let Some(hhmm) = parser::to_hhmm(dep.as_str()) {
                                                    if let Some(now) = current_ts.as_ref() {
                                                        if let Some(delta_min) =
                                                            parser::minutes_until(
                                                                now.as_str(),
                                                                dep.as_str(),
                                                            )
                                                        {
                                                            info!(
                                                                "Departure {}: {} (in {} min) ({})",
                                                                idx + 1,
                                                                hhmm,
                                                                delta_min,
                                                                dep.as_str()
                                                            );
                                                        } else {
                                                            info!(
                                                                "Departure {}: {} ({})",
                                                                idx + 1,
                                                                hhmm,
                                                                dep.as_str()
                                                            );
                                                        }
                                                    } else {
                                                        info!(
                                                            "Departure {}: {} ({})",
                                                            idx + 1,
                                                            hhmm,
                                                            dep.as_str()
                                                        );
                                                    }
                                                } else {
                                                    info!(
                                                        "Departure {}: {}",
                                                        idx + 1,
                                                        dep.as_str()
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        info!(
                                            "API response preview is non-UTF8 ({} bytes shown)",
                                            total_len
                                        );
                                    }
                                } else {
                                    info!("API response: empty body");
                                }
                            }
                            Err(err) => warn!(
                                "Failed while reading API response body: {:?} (buffer={} bytes)",
                                err, API_RESPONSE_PREVIEW_BYTES
                            ),
                        }

                        break;
                    }
                    Ok(Err(err)) => {
                        warn!("API send failed on attempt {}: {:?}", attempt, err);
                    }
                    Err(_) => {
                        warn!(
                            "API send timed out after {}s on attempt {}",
                            HTTP_SEND_TIMEOUT_SECS, attempt
                        );
                    }
                }
            }
            Ok(Err(err)) => {
                warn!(
                    "API request creation failed on attempt {}: {:?}",
                    attempt, err
                );
            }
            Err(_) => {
                warn!(
                    "API request creation timed out after {}s on attempt {}",
                    HTTP_REQUEST_TIMEOUT_SECS, attempt
                );
            }
        }

        if attempt < API_REQUEST_RETRIES {
            Timer::after(Duration::from_secs(API_RETRY_DELAY_SECS)).await;
        }
    }

    let rssi_res = wifi_controller.rssi().expect("Failed to get Wi-Fi RSSI");
    info!("Wi-Fi RSSI: {} dBm", rssi_res);

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }
}
