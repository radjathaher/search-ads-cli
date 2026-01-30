#!/usr/bin/env python3
import argparse
import os
import shutil
import tempfile
import urllib.request
import zipfile
from pathlib import Path

GOOGLEAPIS_ZIP = "https://github.com/googleapis/googleapis/archive/refs/heads/master.zip"


def download_zip(url: str, dest: Path) -> None:
    with urllib.request.urlopen(url) as resp, open(dest, "wb") as f:
        shutil.copyfileobj(resp, f)


def extract_zip(path: Path, out_dir: Path) -> None:
    with zipfile.ZipFile(path, "r") as zf:
        zf.extractall(out_dir)


def copy_tree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(src, dst)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="schemas", help="output dir for proto tree")
    parser.add_argument("--version", default="v19", help="Google Ads API version (e.g. v19)")
    args = parser.parse_args()

    out_dir = Path(args.out).resolve()
    version_dir = args.version

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)

        apis_zip = tmp_path / "googleapis.zip"
        download_zip(GOOGLEAPIS_ZIP, apis_zip)
        extract_zip(apis_zip, tmp_path)

        api_root = tmp_path / "googleapis-master" / "google"
        ads_base = api_root / "ads" / "googleads"
        requested_version = version_dir
        ads_root = ads_base / version_dir
        if not ads_root.exists():
            versions = []
            for entry in ads_base.iterdir():
                if entry.is_dir() and entry.name.startswith("v"):
                    suffix = entry.name[1:]
                    if suffix.isdigit():
                        versions.append((int(suffix), entry.name))
            if not versions:
                raise RuntimeError(f"no versions found under {ads_base}")
            versions.sort()
            latest = versions[-1][1]
            ads_root = ads_base / latest
            version_dir = latest
            print(f"requested {requested_version} not found; using {latest}")

        copy_tree(ads_root, out_dir / "google" / "ads" / "googleads" / version_dir)

        api_dst = out_dir / "google" / "api"
        if api_dst.exists():
            shutil.rmtree(api_dst)
        api_dst.mkdir(parents=True, exist_ok=True)
        for proto in (api_root / "api").glob("*.proto"):
            shutil.copy2(proto, api_dst / proto.name)

        for name in ["rpc", "type", "longrunning"]:
            src = api_root / name
            if src.exists():
                copy_tree(src, out_dir / "google" / name)


if __name__ == "__main__":
    main()
