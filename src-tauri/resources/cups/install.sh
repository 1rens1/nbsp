#!/bin/sh
# Installs (or with "remove", uninstalls) the NBSP virtual CUPS printer. Run as root.
set -e
here=$(dirname "$0")

if [ "$1" = "remove" ]; then
  lpadmin -x NBSP 2>/dev/null || true
  rm -f /usr/lib/cups/backend/nbsp
  exit 0
fi

install -m 0755 "$here/nbsp-backend" /usr/lib/cups/backend/nbsp
install -d -m 0777 /var/spool/nbsp
chmod 0777 /var/spool/nbsp

# SELinux: let cupsd's backends write to the spool dir.
if command -v selinuxenabled >/dev/null 2>&1 && selinuxenabled; then
  if command -v semanage >/dev/null 2>&1; then
    semanage fcontext -a -t print_spool_t '/var/spool/nbsp(/.*)?' 2>/dev/null ||
      semanage fcontext -m -t print_spool_t '/var/spool/nbsp(/.*)?'
    restorecon -R /var/spool/nbsp
  else
    chcon -R -t print_spool_t /var/spool/nbsp
  fi
  restorecon /usr/lib/cups/backend/nbsp 2>/dev/null || true
fi

lpadmin -p NBSP -E -v nbsp:/ -P "$here/nbsp.ppd" -D "NBSP" -L "Opens the job in NBSP"
