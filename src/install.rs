use crate::data::{InstallOptions, Partition};
use regex::Regex;

impl InstallOptions
{
    /// Create the script that applies the settings and installs the system
    pub fn generate_shellscript(&self) -> String
    {
        let mut code = "#/bin/sh\n".to_string();
        code += "# Script automatically generated by jimmy-rs\n\n";
        code += &self.fdisk_cmds().join("\n");
        code += "\n\n";
        code += &self.mkfs_cmds().join("\n");
        code += "\n\n";
        code += &self.mount_cmds().join("\n");
        code += "\n";
        code
    }

    /// Generate a list of shell commands that mount every partition
    fn mount_cmds(&self) -> Vec<String>
    {
        let disks = self.unique_disks_used();

        let mut cmds = Vec::new();
        for disk in disks {
            let partitions = self.partitions_on_disk(&disk);
            let mut i = 0;
            while i < partitions.len() as u32 {
                let cmd = partitions[i as usize].mount_cmd(i);
                if let Some(cmd) = cmd {
                    cmds.push(cmd);
                }
                i += 1;
            }
        }
        cmds
    }

    /// Generate a list of shell commands that format the partitions with `mkfs`, but only for the
    /// partitions whose format has been recognised
    fn mkfs_cmds(&self) -> Vec<String>
    {
        let disks = self.unique_disks_used();

        let mut cmds = Vec::new();
        for disk in disks {
            let partitions = self.partitions_on_disk(&disk);
            let mut i = 0;
            while i < partitions.len() as u32 {
                let cmd = partitions[i as usize].mkfs_cmd(i);
                if let Some(cmd) = cmd {
                    cmds.push(cmd);
                }
                i += 1;
            }
        }
        cmds
    }

    /// Return the list of shell commands that create the partitions with `fdisk`
    fn fdisk_cmds(&self) -> Vec<String>
    {
        let disks = self.unique_disks_used();

        let mut cmds = Vec::new();
        for disk in disks {
            let partitions = self.partitions_on_disk(&disk);

            let mut cmd = String::from("echo -n \"g\\n");
            let mut i = 1;
            while i <= partitions.len() as u32 {
                cmd += partitions[i as usize - 1].fdisk_script_string(i).as_str();
                i += 1;
            }
            cmd += &format!("\\nw\\n\" | fdisk {} &>/dev/null", disk);
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
            // p: primary partition
            // use partition number specified
            // next line: default first sector
            // use partition size specified in instance
            // then: change the type of the partition
            // use the partition number specified
            // change it to the type needed for the format
            r"n\np\n{}\n\n+{}\nt\n{}\n{}\n",
            number,
            &self.size,
            number,
            &self.fdisk_partition_type(),
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
            Some(cmd + " " + &self.get_partition_file(number))
        }
    }

    /// Return a shell command that mounts the given partition
    pub fn mount_cmd(&self, number: u32) -> Option<String>
    {
        if &self.format == "swap" {
            Some(format!(
                "swapon {}",
                self.get_partition_file(number),
            ))
        } else if self.mount.is_empty() {
            None
        } else {
            Some(format!(
                "mkdir -p /mnt{} && mount {} {}",
                self.mount,
                self.get_partition_file(number),
                self.mount,
            ))
        }
    }

    /// Return the path to the partition file (e.g. `/dev/sda1`, if provided `0`, for 0th
    /// partition)
    fn get_partition_file(&self, number: u32) -> String
    {
        let disk = self.disk.clone();
        let n = &(number + 1).to_string();
        // NVME naming patterns deviate from the usual
        let re = Regex::new(r"/dev/nvme\d+n\d+").unwrap();
        if re.is_match(&disk) {
            return disk + "p" + n;
        }
        disk + n
    }

    /// Return the `fdisk` partition type that should be used with the specified format
    fn fdisk_partition_type(&self) -> &str
    {
        match self.format.as_str() {
            "fat32" => "b", // W95 FAT32
            "swap" => "82", // Linux Swap
            _ => "83", // Linux
        }
    }
}
