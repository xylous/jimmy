use serde::Deserialize;

/// *Potentially* valid installation options. Everything is wrapped in `Option<T>` because serde
/// would error if the property isn't found.
#[derive(Deserialize, Debug)]
pub struct ParsedInstallOptions
{
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub locales: Option<Vec<String>>,
    pub kernel: Option<String>,
    pub extra: Option<String>,
    pub bootloader: Option<String>,
    pub partitions: Option<Vec<ParsedPartition>>,
}

/// *Potentially* valid partition options. Everything is wrapped in `Option<T>` because serde would
/// error if the property isn't found.
#[derive(Deserialize, Debug, Clone)]
pub struct ParsedPartition
{
    pub format: Option<String>,
    pub disk: Option<String>,
    pub size: Option<String>,
    pub mount: Option<String>,
}

/// Only the Latest or the LTS kernel can be installed
#[derive(Debug)]
pub enum Kernel {
    Latest,
    Lts,
}

/// Struct that contains the minimum needed to create a functioning Arch installation
#[derive(Debug)]
pub struct InstallOptions
{
    pub username: String,
    pub hostname: String,
    pub region: String,
    pub city: String,
    pub locales: Vec<String>,
    pub kernel: Kernel,
    pub extra: String,
    pub bootloader: String,
    pub partitions: Vec<Partition>,
}

/// If the combination of region and timezone is valid, return true
fn is_valid_zoneinfo(region: Option<String>, city: Option<String>) -> bool
{
    crate::is_file(&format!(
        "/usr/share/zoneinfo/{}{}",
        region.unwrap_or_default(),
        if let Some(city) = city {
            format!("/{}", city)
        } else {
            "".to_string()
        }
    ))
}

impl From<ParsedInstallOptions> for InstallOptions
{
    /// Create a new instance of `InstallOptions` from an instance of `ParsedInstallOptions`
    fn from(raw: ParsedInstallOptions) -> Self
    {
        let kernel = match raw.kernel.unwrap_or_default().as_str() {
            "latest" => Kernel::Latest,
            _ => Kernel::Lts, // assume LTS kernel at all times
        };
        let locales =
            if let Some(l) = raw.locales {
                match l[..] {
                    [] => {
                        eprintln!("warning: locales not specified; defaulting to 'en_US.UTF-8'");
                        vec!["en_US.UTF-8".to_string()]
                    }
                    _ => l
                }
            } else {
                eprintln!("warning: locales not specified; defaulting to 'en_US.UTF-8'");
                vec!["en_US.UTF-8".to_string()]
            };

        if !is_valid_zoneinfo(raw.region.clone(), raw.city.clone()) {
            panic!("invalid zoneinfo (region: '{:?}', city: '{:?}'", raw.region, raw.city);
        }

        Self {
            username: raw.username.expect("error: username not specified"),
            hostname: raw.hostname.expect("error: hostname not specified"),
            region: raw.region.unwrap_or_default(),
            city: raw.city.unwrap_or_default(),
            locales,
            kernel,
            extra: raw.extra.unwrap_or_default(),
            bootloader: raw.bootloader.expect("error: no bootloader specified"),
            // turn every `ParsedPartition` into a proper `Partition`
            partitions: raw.partitions.expect("error: no partitions specified")
                            .into_iter().map(|p| p.into()).collect(),
        }
    }
}

/// Struct that contains the minimum needed to create a partition on disk
#[derive(Debug)]
pub struct Partition
{
    pub format: String,
    pub disk: String,
    pub size: String,
    pub mount: String,
}

impl From<ParsedPartition> for Partition
{
    /// Create a new instance of `Partition` from an instance of `ParsedPartition`
    fn from(raw: ParsedPartition) -> Self
    {
        let format: String;
        if raw.format.is_none() || raw.format.as_ref().unwrap() == "" {
            eprintln!("warning: partition format not specified; defaulting to 'ext4'");
            format = "ext4".to_string();
        } else {
            format = raw.format.unwrap();
        }
        if raw.mount.is_none() || raw.mount.as_ref().unwrap() == "" {
            eprintln!("warning: partition mount not specified; it's not going to be mounted");
        }
        Self {
            format,
            disk: raw.disk.expect("error: partition disk not specified"),
            size: raw.size.unwrap_or_else(|| "".to_string()),
            mount: raw.mount.unwrap_or_else(|| "".to_string()),
        }
    }
}

#[allow(dead_code)]
pub fn sample_input_file() -> &'static str
{
r"# Basic arch installation; latest kernel with a single root partition, booted
# with GRUB
# It uses /dev/sda for its partition

username: arch-user-btw
hostname: archlinux

# user preferences
bootloader: grub
extra: vim

# Timezone info, as per /usr/share/zoneinfo/*Region*/*City*
# For example purpoeses, use London, Europe
region: Europe
city: London

# List of locales to use and generate. By default, when nothing is specified,
# 'en_US.UTF-8' is assumed.
locales:
  - en_US.UTF-8

# alternatively: `lts`
kernel: latest

# you have to configure partitions manually
partitions:
  # the name of the array serves no purpose other than readability
  - root:
    format: ext4
    mount: /
    disk: /dev/sda
    # when there's no `size` property, it's assumed you want the remaining space
    # on the disk
"
}
