> honestly, /etc/packages.list is the way to go

● YES! /etc/packages.list is SO CLEAN!

  # /etc/packages.list
  nginx
  postgresql-15
  redis-server
  htop
  # Web servers only
  [[webservers]] certbot
  [[webservers]] php8.2-fpm
  # Database servers
  [[databases]] postgresql-15-replication
  [[databases]] pgbackrest

  Then just:
  laszoo watch packages /etc/packages.list --exec="apt-get update && cat /etc/packages.list | grep -v '^#' | xargs apt-get install -y"

