; Let LAN devices reach the local proxy when "Allow LAN" is on.
;
; In that mode the core (airport-core.exe) binds 0.0.0.0, but Windows Firewall
; blocks inbound by default, so LAN devices cannot use this machine as a proxy
; (clash-verge-rev #5726). We add a program-scoped inbound allow rule, limited to
; the local subnet, at install and remove it at uninstall. The rule is inert when
; "Allow LAN" is off, because the core then binds 127.0.0.1 only. The installer
; runs elevated, so netsh needs no extra prompt. Covers system-proxy mode; TUN
; mode serves its inbound from the privileged service's own core copy, so LAN use
; of TUN mode is not covered here.
;
; Separately, LAN file transfer (lan-service) runs inside the main app process
; (ClashNova.exe): it listens on UDP 53317 for multicast discovery and HTTP/HTTPS
; 53317 for transfers. Windows Firewall blocks that inbound by default too, so
; without an allow rule peers can neither discover nor send to this machine. We add
; a second program-scoped inbound rule for the main app, also limited to the local
; subnet. Program-scoped covers both the UDP discovery and the TCP transfer port.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Delete first so repeated installs/updates do not stack duplicate rules.
  nsExec::Exec 'netsh advfirewall firewall delete rule name="ClashNova LAN Proxy"'
  Pop $0
  nsExec::Exec 'netsh advfirewall firewall add rule name="ClashNova LAN Proxy" dir=in action=allow program="$INSTDIR\airport-core.exe" enable=yes profile=any remoteip=localsubnet'
  Pop $0
  ; LAN file transfer listener (UDP discovery + TCP transfer on 53317).
  nsExec::Exec 'netsh advfirewall firewall delete rule name="ClashNova LAN Transfer"'
  Pop $0
  nsExec::Exec 'netsh advfirewall firewall add rule name="ClashNova LAN Transfer" dir=in action=allow program="$INSTDIR\ClashNova.exe" enable=yes profile=any remoteip=localsubnet'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="ClashNova LAN Proxy"'
  Pop $0
  nsExec::Exec 'netsh advfirewall firewall delete rule name="ClashNova LAN Transfer"'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
