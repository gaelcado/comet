# Mobile rows at 15af555a

Source: fix/harness-update-lifecycle at 15af555a, rebased onto bbf590c8.
Executable: /tmp/pr389-ios-evidence/Build/Products/Debug-iphonesimulator/Zeron.app/Zeron.
iPhone 17 Pro / iOS 26.3, simulator 4FC55F0A-1A0E-4088-9736-D2CAB560B01A. Dark appearance, default text size, synthetic demo data.
Native framebuffer captures: -demo -route updates:dev-mac; -demo -sheet devices.
Both UI tests passed after row sizing correction. The strengthened available-row hittability test also passed. Desktop UI compile passed after rebase; workspace formatting reports unrelated drift.
Other sizes, RTL and accessibility text sizes not verified in this iteration.
