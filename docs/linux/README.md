# Linux systemd Services

## Switch Off LCD on Boot

The systemd unit [lcd-off.service](https://github.com/zehnm/aoostar-rs/blob/main/linux/lcd-off.service)
can be installed to automatically switch off the embedded LCD on boot.

The unit file has been tailored to Debian-based Linux distros and has been tested on Proxmox 8.4 and Ubuntu 25.04.

Requirements:
- `/dev/ttyACM0`: `dialout` group with rw permissions. 
  - To run as root: remove `DynamicUser` and `Group` settings. 

### Install

As root user, otherwise `sudo` is required:
```shell
cp asterctl /usr/local/bin/
cp lcd-off.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable lcd-off
```

### Security

The systemd unit file uses strong security settings to only allow operations required for `asterctl`:

```shell
systemd-analyze security lcd-off.service
```

<details>

```
  NAME                                                        DESCRIPTION                                                                    EXPOSURE
✓ SystemCallFilter=~@swap                                     System call allow list defined for service, and @swap is not included                  
✓ SystemCallFilter=~@resources                                System call allow list defined for service, and @resources is not included             
✓ SystemCallFilter=~@reboot                                   System call allow list defined for service, and @reboot is not included                
✓ SystemCallFilter=~@raw-io                                   System call allow list defined for service, and @raw-io is not included                
✓ SystemCallFilter=~@privileged                               System call allow list defined for service, and @privileged is not included            
✓ SystemCallFilter=~@obsolete                                 System call allow list defined for service, and @obsolete is not included              
✓ SystemCallFilter=~@mount                                    System call allow list defined for service, and @mount is not included                 
✓ SystemCallFilter=~@module                                   System call allow list defined for service, and @module is not included                
✓ SystemCallFilter=~@debug                                    System call allow list defined for service, and @debug is not included                 
✓ SystemCallFilter=~@cpu-emulation                            System call allow list defined for service, and @cpu-emulation is not included         
✓ SystemCallFilter=~@clock                                    System call allow list defined for service, and @clock is not included                 
✓ RemoveIPC=                                                  Service user cannot leave SysV IPC objects around                                      
✗ RootDirectory=/RootImage=                                   Service runs within the host's root directory                                       0.1
✓ User=/DynamicUser=                                          Service runs under a transient non-root user identity                                  
✓ RestrictRealtime=                                           Service realtime scheduling access is restricted                                       
✓ CapabilityBoundingSet=~CAP_SYS_TIME                         Service processes cannot change the system clock                                       
✓ NoNewPrivileges=                                            Service processes cannot acquire new privileges                                        
✓ AmbientCapabilities=                                        Service process does not receive ambient capabilities                                  
✗ PrivateDevices=                                             Service potentially has access to hardware devices                                  0.2
✓ CapabilityBoundingSet=~CAP_BPF                              Service may not load BPF programs                                                      
✗ SystemCallArchitectures=                                    Service may execute system calls with all ABIs                                      0.2
✓ ProtectSystem=                                              Service has strict read-only access to the OS file hierarchy                           
✓ ProtectProc=                                                Service has restricted access to process tree (/proc hidepid=)                         
✓ SupplementaryGroups=                                        Service has no supplementary groups                                                    
✓ CapabilityBoundingSet=~CAP_SYS_RAWIO                        Service has no raw I/O access                                                          
✓ CapabilityBoundingSet=~CAP_SYS_PTRACE                       Service has no ptrace() debugging abilities                                            
✓ CapabilityBoundingSet=~CAP_SYS_(NICE|RESOURCE)              Service has no privileges to change resource use parameters                            
✓ CapabilityBoundingSet=~CAP_NET_ADMIN                        Service has no network configuration privileges                                        
✓ CapabilityBoundingSet=~CAP_NET_(BIND_SERVICE|BROADCAST|RAW) Service has no elevated networking privileges                                          
✗ DeviceAllow=                                                Service has no device ACL                                                           0.2
✓ CapabilityBoundingSet=~CAP_AUDIT_*                          Service has no audit subsystem access                                                  
✓ CapabilityBoundingSet=~CAP_SYS_ADMIN                        Service has no administrator privileges                                                
✓ PrivateNetwork=                                             Service has no access to the host's network                                            
✓ PrivateTmp=                                                 Service has no access to other software's temporary files                              
✓ ProcSubset=                                                 Service has no access to non-process /proc files (/proc subset=)                       
✓ CapabilityBoundingSet=~CAP_SYSLOG                           Service has no access to kernel logging                                                
✓ ProtectHome=                                                Service has no access to home directories                                              
✓ KeyringMode=                                                Service doesn't share key material with other services                                 
✓ Delegate=                                                   Service does not maintain its own delegated control group subtree                      
✓ PrivateUsers=                                               Service does not have access to other users                                            
✗ IPAddressDeny=                                              Service does not define an IP address allow list                                    0.2
✓ NotifyAccess=                                               Service child processes cannot alter service state                                     
✓ ProtectClock=                                               Service cannot write to the hardware clock or system clock                             
✓ CapabilityBoundingSet=~CAP_SYS_PACCT                        Service cannot use acct()                                                              
✓ CapabilityBoundingSet=~CAP_KILL                             Service cannot send UNIX signals to arbitrary processes                                
✓ ProtectKernelLogs=                                          Service cannot read from or write to the kernel log ring buffer                        
✓ CapabilityBoundingSet=~CAP_WAKE_ALARM                       Service cannot program timers that wake up the system                                  
✓ CapabilityBoundingSet=~CAP_(DAC_*|FOWNER|IPC_OWNER)         Service cannot override UNIX file/IPC permission checks                                
✓ ProtectControlGroups=                                       Service cannot modify the control group file system                                    
✓ CapabilityBoundingSet=~CAP_LINUX_IMMUTABLE                  Service cannot mark files immutable                                                    
✓ CapabilityBoundingSet=~CAP_IPC_LOCK                         Service cannot lock memory into RAM                                                    
✓ ProtectKernelModules=                                       Service cannot load or read kernel modules                                             
✓ CapabilityBoundingSet=~CAP_SYS_MODULE                       Service cannot load kernel modules                                                     
✓ CapabilityBoundingSet=~CAP_SYS_TTY_CONFIG                   Service cannot issue vhangup()                                                         
✓ CapabilityBoundingSet=~CAP_SYS_BOOT                         Service cannot issue reboot()                                                          
✓ CapabilityBoundingSet=~CAP_SYS_CHROOT                       Service cannot issue chroot()                                                          
✓ PrivateMounts=                                              Service cannot install system mounts                                                   
✓ CapabilityBoundingSet=~CAP_BLOCK_SUSPEND                    Service cannot establish wake locks                                                    
✓ MemoryDenyWriteExecute=                                     Service cannot create writable executable memory mappings                              
✓ RestrictNamespaces=~user                                    Service cannot create user namespaces                                                  
✓ RestrictNamespaces=~pid                                     Service cannot create process namespaces                                               
✓ RestrictNamespaces=~net                                     Service cannot create network namespaces                                               
✓ RestrictNamespaces=~uts                                     Service cannot create hostname namespaces                                              
✓ RestrictNamespaces=~mnt                                     Service cannot create file system namespaces                                           
✓ CapabilityBoundingSet=~CAP_LEASE                            Service cannot create file leases                                                      
✓ CapabilityBoundingSet=~CAP_MKNOD                            Service cannot create device nodes                                                     
✓ RestrictNamespaces=~cgroup                                  Service cannot create cgroup namespaces                                                
✓ RestrictNamespaces=~ipc                                     Service cannot create IPC namespaces                                                   
✓ ProtectHostname=                                            Service cannot change system host/domainname                                           
✓ CapabilityBoundingSet=~CAP_(CHOWN|FSETID|SETFCAP)           Service cannot change file ownership/access mode/capabilities                          
✓ CapabilityBoundingSet=~CAP_SET(UID|GID|PCAP)                Service cannot change UID/GID identities/capabilities                                  
✓ LockPersonality=                                            Service cannot change ABI personality                                                  
✓ ProtectKernelTunables=                                      Service cannot alter kernel tunables (/proc/sys, …)                                    
✓ RestrictAddressFamilies=~AF_PACKET                          Service cannot allocate packet sockets                                                 
✓ RestrictAddressFamilies=~AF_NETLINK                         Service cannot allocate netlink sockets                                                
✓ RestrictAddressFamilies=~AF_UNIX                            Service cannot allocate local sockets                                                  
✓ RestrictAddressFamilies=~…                                  Service cannot allocate exotic sockets                                                 
✓ RestrictAddressFamilies=~AF_(INET|INET6)                    Service cannot allocate Internet sockets                                               
✓ CapabilityBoundingSet=~CAP_MAC_*                            Service cannot adjust SMACK MAC                                                        
✓ RestrictSUIDSGID=                                           SUID/SGID file creation by service is restricted                                       
✓ UMask=                                                      Files created by service are accessible only by service's own user by default          
```

</details>

```
→ Overall exposure level for lcd-off.service: 0.8 SAFE 😀
```

## asterctl Daemon

_TODO_

## aster-sysinfo Daemon

‼️ Work in progress. This also requires the `asterctl` daemon running. 

The systemd unit [aster-sysinfo.service](https://github.com/zehnm/aoostar-rs/blob/main/linux/aster-sysinfo.service)
can be installed to automatically collect system sensor information for `asterctl`.

### Install

As root user, otherwise `sudo` is required:
```shell
cp aster-sysinfo /usr/local/bin/
cp aster-sysinfo.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable aster-sysinfo
```
