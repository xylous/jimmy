use crate::data::{InstallOptions, Partition, Kernel};
use regex::Regex;

/// Take the second element of each of the tuples in the input only if they're Some()
fn map_snd<A, B>(tuples: Vec<(A, Option<B>)>) -> Vec<B>
{
    tuples.into_iter()
        .filter_map(|f| {
            f.1
        })
        .collect()
}

impl InstallOptions
{
    /// Create the script that applies the settings and installs the system
    pub fn generate_shellscript(&self) -> String
    {
        let lines: Vec<String> = vec![
            "#!/bin/sh\n# Script automatically generated by jimmy-rs",
            "timedatectl set-ntp true",
            &self.fdisk_cmds().join("\n"),
            &map_snd(self.map_partitions(Partition::mkfs_cmd)).join("\n"),
            &map_snd(self.map_partitions(Partition::mount_cmd)).join("\n"),
            &("echo 'Y' | pacstrap /mnt ".to_owned() + &self.packages().join(" ")),
            "genfstab -U /mnt >> /mnt/etc/fstab",
            // The system configuration part is a bit complicated, since we first need to create a
            // different script, put it in /mnt, run it with arch-chroot, and then delete it after
            // we're done.
            // Check `https://bbs.archlinux.org/viewtopic.php?id=204252`
            &("cat <<END_OF_SECOND_SCRIPT > ".to_owned() + "/mnt/jimmy_part2.sh\n" +
              &self.chroot_script() + "END_OF_SECOND_SCRIPT"),
            "chmod +x /mnt/jimmy_part2.sh",
            &("arch-chroot /mnt ./jimmy_part2.sh\n".to_owned() +
              "rm -f /mnt/jimmy_part2.sh"),
            "umount -R /mnt",
        ].iter().map(|s| s.to_string()).collect();
        lines.join("\n\n") + "\n"
    }

    /// Create the script that is ran from inside the arch-chroot session to configure the system
    fn chroot_script(&self) -> String
    {
        let lines: Vec<String> = vec![
            "#!/bin/sh\n# arch-chroot script automatically generated by jimmy-rs",
            &format!(
                "ln -sf /usr/share/zoneinfo/{}/{} /etc/localtime\nhwclock --systohc",
                self.region,
                self.city,
            ),
            &(self.locales_cmd().join("\n") + "\n" + "locale-gen"),
            &format!("echo '{}' >/etc/hostname", &self.hostname),
            &self.local_hostname_cmd(),
            &InstallOptions::configure_networkmanager().join("\n"),
            &("echo 'Set root password:'\n".to_owned() + "passwd"),
            &self.install_bootloader().join("\n"),
            "exit",
        ].iter().map(|s| s.to_string()).collect();
        lines.join("\n\n") + "\n"
    }

    /// Return a list of commands that get the specified bootloader up and running, or panic if the
    /// bootloader isn't valid
    fn install_bootloader(&self) -> Vec<String>
    {
        match self.bootloader.as_str() {
            "grub" =>
                vec![
                    "grub-install --target=x86_64-efi --bootloader-id=GRUB --recheck",
                    "grub-mkconfig -o /boot/grub/grub.cfg",
                ].into_iter().map(|s| s.to_string()).collect(),
            "efistub" => {
                let lts = match &self.kernel {
                    Kernel::Lts => "-lts",
                    _ => "",
                };
                let partitions_and_disks = self.map_partitions(Partition::get_partition_file);
                let boot_partition = partitions_and_disks.iter()
                    .find(|(p, _)| matches!(p.mount.as_str(), "/boot" | "/efi"))
                    .expect("using efistub, but no boot partition was detected");
                let part_re = Regex::new(r"\d+$").unwrap();
                let root_partition = partitions_and_disks.iter()
                    .find(|(p, _)| p.mount.as_str() == "/")
                    .expect("using efistub, but no root partition was detected");

                vec![
                    format!(
                        "efibootmgr --disk {} --part {} --create --label \"Arch Linux{}\" --loader /vmlinuz-linux{} --unicode='root={} rw initrd=\\initramfs-linux{}.img' --verbose",
                        boot_partition.0.disk,
                        part_re.find(&boot_partition.1.clone().unwrap()).map(|s| s.as_str()).unwrap_or(""),
                        match lts { // if using LTS kernel, then put label "Arch Linux LTS"
                            "-lts" => " LTS",
                            _ => ""
                        },
                        lts, // if using LTS kernel, use /vmlinuz-linux-lts
                        root_partition.1.clone().unwrap(), // find root partition
                        lts, // if using LTS kernel, use \initramfs-linux-lts.img
                    )
                ]
                },
            _ => panic!("invalid bootloader"),
        }
    }

    /// Return a command that creates /etc/hosts and puts local hostname information into it
    fn local_hostname_cmd(&self) -> String
    {
        format!(
            "cat <<END_ETC_HOSTS >/etc/hosts\n{}\nEND_ETC_HOSTS",
            vec![
                "127.0.0.1\tlocalhost",
                "::1\tlocalhost",
                &format!("127.0.1.1\t{}", &self.hostname),
            ].join("\n"),
        )
    }

    /// Return a list of commands that get NetworkManager up and running. This assumes, of course,
    /// that it's installed
    fn configure_networkmanager() -> Vec<&'static str>
    {
        vec![
            "systemctl enable --now systemd-resolved",
            "systemctl enable NetworkManager.service",
        ]
    }

    /// Return a vector containing the sed command that sets (uncomments) all specified locales in
    /// /etc/locale.gen, and the command that creates /etc/locale.conf and puts `LANG=${first of
    /// the locales}` into it
    fn locales_cmd(&self) -> Vec<String>
    {
        let mut fst = vec!["sed ".to_string()];
        for l in self.locales.clone() {
            fst.push(format!("    --expression 's/^#{}$/{}/' ",
                                l,
                                l,
                                ));
        }
        fst.push("    --in-place /etc/locale.gen".to_string());
        vec![
            fst.join("\\\n"),
            format!("echo 'LANG={}' >/etc/locale.conf", self.locales.clone()[0]),
        ]
    }

    /// Return a list of packages that need to be installed with `pacstrap` onto the new system
    fn packages(&self) -> Vec<&str>
    {
        vec![
            "base",
            match self.kernel {
                Kernel::Latest => "linux",
                Kernel::Lts => "linux-lts",
            },
            "linux-firmware",
            &self.extra,
            &self.bootloader,
            "efibootmgr",
            "networkmanager",
        ]
    }

    /// Map a function `apply()` over all partitions, by associating them with their disks so that
    /// the proper file paths are used to identify them. The result of that function is added to
    /// the return value only if it's `Some()`
    fn map_partitions(&self, apply: fn(&Partition, u32) -> Option<String>) -> Vec<(&Partition, Option<String>)>
    {
        let disks = self.unique_disks_used();

        disks.iter().map(|disk| {
            let partitions = self.partitions_on_disk(disk).into_iter();

            partitions
                .enumerate()
                .map(|(idx, partition)| {
                    (partition, apply(partition, idx as u32))
                })
                .collect::<Vec<(&Partition, Option<String>)>>()
        })
        .flatten()
        .collect::<Vec<(&Partition, Option<String>)>>()
    }

    /// TODO: find a way to make this function use `map_partitions()`
    /// Return the list of shell commands that create the partitions with `fdisk`
    fn fdisk_cmds(&self) -> Vec<String>
    {
        let disks = self.unique_disks_used();

        let mut cmds = Vec::new();
        for disk in disks {
            let partitions = self.partitions_on_disk(&disk);

            let mut cmd = String::from("echo -e \"g\\n");
            let mut i = 1;
            while i <= partitions.len() as u32 {
                cmd += partitions[i as usize - 1].fdisk_script_string(i).as_str();
                i += 1;
            }
            cmd += &format!("\\nw\" | fdisk {} &>/dev/null", disk);
            cmds.push(cmd);
        }
        cmds
    }

    /// Return the list of all unique disks used in the configuration
    fn unique_disks_used(&self) -> Vec<String>
    {
        let mut disks: Vec<String> = self.partitions.iter()
            .map(|p| p.disk.clone())
            .collect();
        disks.sort();
        disks.dedup();
        disks
    }

    /// Given the name of a disk, return a list of all partitions that would be on it
    fn partitions_on_disk(&self, disk: &str) -> Vec<&Partition>
    {
        self.partitions
            .iter()
            .filter(|x| x.disk == disk)
            .collect()
    }
}

impl Partition
{
    /// Return the string that can be `echo`ed into `fdisk` to create this Partition
    pub fn fdisk_script_string(&self, number: u32) -> String
    {
        format!(
            // n: create new partition
            // use partition number specified
            // next line: default first sector
            // use partition size specified in instance
            // then: change the type of the partition
            // use the partition number specified
            // change it to the type needed for the format
            r"n\n{}\n\n{}\nt{}\n{}\n",
            number,
            if self.size.is_empty() {
                "".to_string()
            } else {
                format!("+{}", &self.size)
            },
            // The first partition is going to be selected by default
            if number == 1 {
                "".to_string()
            } else {
                format!("\\n{}", number)
            },
            self.fdisk_partition_type()
        )
    }

    /// Return the `mkfs` command that can format this partition, or `None` if the format of the
    /// partition wasn't recognised.
    pub fn mkfs_cmd(&self, number: u32) -> Option<String>
    {
        let cmd = match self.format.as_str() {
            "ext2" => "mkfs.ext2",
            "ext3" => "mkfs.ext3",
            "ext4" => "mkfs.ext4",
            "fat32" => "mkfs.fat -F 32",
            "swap" => "mkswap",
            _ => ""
        }.to_string();
        if cmd.is_empty() { // if true, then we didn't recognise the format
            None
        } else {
            Some(cmd + " " + &self.get_partition_file(number).unwrap())
        }
    }

    /// Return a shell command that mounts the given partition
    pub fn mount_cmd(&self, number: u32) -> Option<String>
    {
        if &self.format == "swap" {
            Some(format!(
                "swapon {}",
                self.get_partition_file(number).unwrap(),
            ))
        } else if self.mount.is_empty() {
            None
        } else {
            Some(format!(
                "mkdir -p /mnt{} && mount {} /mnt{}",
                self.mount,
                self.get_partition_file(number).unwrap(),
                self.mount,
            ))
        }
    }

    /// Return the path to the partition file (e.g. `/dev/sda1`, if provided `0`, for 0th
    /// partition)
    fn get_partition_file(&self, number: u32) -> Option<String>
    {
        let disk = self.disk.clone();
        let n = &(number + 1).to_string();
        // NVME naming patterns deviate from the usual
        let re = Regex::new(r"/dev/nvme\d+n\d+").unwrap();
        Some(if re.is_match(&disk) {
            disk + "p" + n
        } else {
            disk + n
        })
    }

    /// Return the `fdisk` partition type that should be used with the specified format
    fn fdisk_partition_type(&self) -> &str
    {
        match self.format.as_str() {
            "fat32" => "uefi", // EFI System
            "swap" => "swap", // Linux swap
            _ => "linux", // Linux filesystem
        }
    }
}
