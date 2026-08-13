import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();

const readText = (path: string) => readFileSync(resolve(root, path), "utf8");

const readBitmapMetadata = (path: string) => {
  const bitmap = readFileSync(resolve(root, path));

  return {
    bitsPerPixel: bitmap.readUInt16LE(28),
    height: Math.abs(bitmap.readInt32LE(22)),
    signature: bitmap.toString("ascii", 0, 2),
    width: bitmap.readInt32LE(18),
  };
};

describe("Windows installer packaging", () => {
  it("uses the branded current-user NSIS installer", () => {
    const config = JSON.parse(readText("src-tauri/tauri.conf.json"));

    expect(config.bundle.windows.nsis).toEqual({
      compression: "lzma",
      displayLanguageSelector: false,
      headerImage: "nsis/header.bmp",
      installMode: "currentUser",
      installerHooks: "nsis/installer-hooks.nsh",
      installerIcon: "icons/icon.ico",
      languages: ["English", "SimpChinese", "TradChinese", "Japanese"],
      sidebarImage: "nsis/sidebar.bmp",
      template: "nsis/installer.nsi",
    });
  });

  it("ships valid 24-bit NSIS branding bitmaps", () => {
    expect(readBitmapMetadata("src-tauri/nsis/header.bmp")).toEqual({
      bitsPerPixel: 24,
      height: 57,
      signature: "BM",
      width: 150,
    });
    expect(readBitmapMetadata("src-tauri/nsis/sidebar.bmp")).toEqual({
      bitsPerPixel: 24,
      height: 314,
      signature: "BM",
      width: 164,
    });
  });

  it("publishes NSIS as the Windows updater and keeps MSI as a fallback", () => {
    const workflow = readText(".github/workflows/release.yml");

    expect(workflow).toContain("AgentSwitch-$VERSION-Windows-Setup.exe");
    expect(workflow).toContain("Signature not found for $($nsis.Name)");
    expect(workflow).toContain("*-Windows-Setup.exe)");
    expect(workflow).toContain("*.msi)");
  });

  it("migrates legacy per-user MSI installs and preserves their directory", () => {
    const installer = readText("src-tauri/nsis/installer.nsi");
    const restoreInstallLocation = installer.slice(
      installer.indexOf("Function RestorePreviousInstallLocation"),
      installer.indexOf(
        "Function Skip",
        installer.indexOf("Function RestorePreviousInstallLocation"),
      ),
    );

    expect(installer).toContain(
      'EnumRegKey $1 HKCU "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"',
    );
    expect(installer).toContain(
      'ReadRegStr $4 HKCU "${MANUPRODUCTKEY}" "InstallDir"',
    );
    expect(installer).toContain('StrCmp "$R0$R1" "CC Switch${MANUFACTURER}"');
    expect(installer).toContain(
      'StrCpy $INSTDIR "$LOCALAPPDATA\\Programs\\${PRODUCTNAME}"',
    );
    expect(installer).toContain(
      'EnumRegKey $1 HKLM "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"',
    );
    expect(restoreInstallLocation).not.toContain("ReadRegStr $4 HKLM");
    expect(installer).not.toContain("MUI_PAGE_DIRECTORY");
    expect(readText("src-tauri/nsis/installer-hooks.nsh")).toContain(
      "Function WelcomePageShow",
    );
  });
});
