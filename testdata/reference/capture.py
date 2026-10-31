import math
import random
import shutil
import subprocess
import tempfile
from pathlib import Path

from playwright.sync_api import sync_playwright

HERE = Path(__file__).parent
URL = (HERE / "article.html").resolve().as_uri()


def video(page) -> int:
    total = page.evaluate("document.documentElement.scrollHeight - window.innerHeight")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        pos, k = 0.0, 0
        page.evaluate("window.scrollTo(0, 0)")
        while True:
            page.screenshot(path=tmp / f"{k:05d}.png")
            k += 1
            if pos >= total:
                for _ in range(10):
                    page.screenshot(path=tmp / f"{k:05d}.png")
                    k += 1
                break
            pos = min(total, pos + 12.2 * (1 - math.cos(2 * math.pi * k / 48.5)))
            page.evaluate(f"window.scrollTo(0, {round(pos)})")
        mp4 = tmp / "video.mp4"
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-framerate", "25", "-i", str(tmp / "%05d.png"),
             "-c:v", "libx264", "-preset", "veryfast", "-crf", "26", "-pix_fmt", "yuv420p", str(mp4)],
            check=True,
        )
        out = HERE / "frames_video"
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir()
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(mp4), "-vf", "fps=5", "-q:v", "3",
                        str(out / "%04d.jpg")], check=True)
    return len(list(out.iterdir()))


def run() -> None:
    random.seed(7)
    with sync_playwright() as p:
        browser = p.chromium.launch()
        for scale in (1.0, 1.25):
            tag = f"{int(scale * 100)}"
            page = browser.new_page(viewport={"width": 1280, "height": 760}, device_scale_factor=scale)
            page.goto(URL)
            page.wait_for_load_state("networkidle")
            page.screenshot(path=HERE / f"full_{tag}.png", full_page=True)
            if scale == 1.0:
                page.screenshot(path=HERE / "snip_top.png")
                page.evaluate("window.scrollTo(0, 1500)")
                page.screenshot(path=HERE / "snip_mid.png", clip={"x": 240, "y": 60, "width": 800, "height": 640})
                page.evaluate("window.scrollTo(0, 0)")

            out = HERE / f"frames_{tag}"
            shutil.rmtree(out, ignore_errors=True)
            out.mkdir()
            total = page.evaluate("document.documentElement.scrollHeight - window.innerHeight")
            pos, i = 0, 0
            while True:
                page.screenshot(path=out / f"{i:04d}.png")
                i += 1
                if random.random() < 0.15:
                    page.screenshot(path=out / f"{i:04d}.png")
                    i += 1
                if pos >= total:
                    break
                pos = min(total, pos + random.randint(60, 420))
                page.evaluate(f"window.scrollTo(0, {pos})")
                page.wait_for_timeout(30)
            print(f"{tag}%: {i} frames, total scroll {total}px")
            if scale == 1.0:
                print(f"video: {video(page)} frames")
            page.close()
        browser.close()


if __name__ == "__main__":
    run()
