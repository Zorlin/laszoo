[Unit]
Description=MooseFS Laszoo mount
After=network-online.target
Wants=network-online.target

[Mount]
What=mfsmount
Where=/mnt/laszoo
Type=fuse
Options=mfsmaster=mfsmaster.lon.riff.cc,mfsdelayedinit,_netdev,mfspassword={{ mfspassword }},mfssubfolder=/laszoo

[Install]
WantedBy=multi-user.target
