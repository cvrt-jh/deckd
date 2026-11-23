//! Wi-Fi station from credentials in NVS.
//!
//! `tools/provision.sh` writes namespace `deckpanel` with `wifi_ssid`,
//! `wifi_psk` and `panel_name`. Nothing here ever logs the PSK.

use anyhow::{anyhow, bail, Context as _, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// NVS namespace of the device settings.
pub const NVS_NAMESPACE: &str = "deckpanel";
/// NVS key of the Wi-Fi SSID (at most 32 bytes).
pub const KEY_SSID: &str = "wifi_ssid";
/// NVS key of the Wi-Fi PSK (at most 64 bytes).
pub const KEY_PSK: &str = "wifi_psk";
/// NVS key of the panel name, e.g. `jh-desk`.
pub const KEY_PANEL_NAME: &str = "panel_name";

/// Room for the longest value plus the NUL terminator NVS stores.
const NVS_STR_BUF: usize = 72;

/// Wi-Fi credentials read from NVS.
struct Credentials {
    ssid: String,
    psk: String,
}

/// Connect to the network stored in NVS and wait for an IP address.
///
/// Returns the driver; dropping it disconnects.
///
/// # Errors
/// Fails with `no credentials in NVS (deckpanel/wifi_ssid)` when the panel
/// is not provisioned, and on any driver or connection error.
pub fn connect(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<EspWifi<'static>> {
    let settings = EspNvs::<NvsDefault>::new(nvs.clone(), NVS_NAMESPACE, true)
        .with_context(|| format!("open NVS namespace {NVS_NAMESPACE}"))?;
    if let Some(name) = read_str(&settings, KEY_PANEL_NAME)? {
        log::info!("wifi: panel name {name}");
    }
    let creds = read_credentials(&settings)?;

    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs)).context("create driver")?;
    let config = Configuration::Client(ClientConfiguration {
        ssid: creds
            .ssid
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("SSID longer than 32 bytes"))?,
        password: creds
            .psk
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("PSK longer than 64 bytes"))?,
        auth_method: if creds.psk.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    });
    {
        let mut blocking = BlockingWifi::wrap(&mut wifi, sysloop).context("wrap driver")?;
        blocking
            .set_configuration(&config)
            .context("set configuration")?;
        blocking.start().context("start")?;
        log::info!("wifi: connecting to {}", creds.ssid);
        blocking.connect().context("connect")?;
        blocking.wait_netif_up().context("wait for IP")?;
    }
    let netif = wifi.sta_netif();
    let ip = netif.get_ip_info().context("read IP info")?.ip;
    let mac = netif.get_mac().context("read MAC")?;
    log::info!("wifi: connected, ip {ip}, mac {}", format_mac(mac));
    Ok(wifi)
}

fn read_credentials(settings: &EspNvs<NvsDefault>) -> Result<Credentials> {
    let Some(ssid) = read_str(settings, KEY_SSID)? else {
        bail!("no credentials in NVS ({NVS_NAMESPACE}/{KEY_SSID})");
    };
    let Some(psk) = read_str(settings, KEY_PSK)? else {
        bail!("no credentials in NVS ({NVS_NAMESPACE}/{KEY_PSK})");
    };
    Ok(Credentials { ssid, psk })
}

fn read_str(settings: &EspNvs<NvsDefault>, key: &str) -> Result<Option<String>> {
    let mut buf = [0u8; NVS_STR_BUF];
    let value = settings
        .get_str(key, &mut buf)
        .with_context(|| format!("read NVS {NVS_NAMESPACE}/{key}"))?;
    Ok(value.map(str::to_owned))
}

/// `aa:bb:cc:dd:ee:ff`.
#[must_use]
pub fn format_mac(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
