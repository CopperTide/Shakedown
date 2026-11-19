"""What a world needs from the machine: the Docker images of its systems (checked, never pulled) and its
memory."""
import subprocess

from .choices import problem


def image_present(image: str) -> bool:
    """True when the local Docker has `image`; False when it doesn't or there is no Docker."""
    try:
        done = subprocess.run(["docker", "image", "inspect", "--format", "{{.Id}}", image], capture_output=True,
                              timeout=20, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return False
    return done.returncode == 0


def of(systems: list[str], t) -> dict:
    """`{images: [{image, present, hint}], memory_mb}` of the chosen `systems` (images once each)."""
    chosen = [s for s in t.systems if s["id"] in systems]
    images: dict[str, dict] = {}
    for s in chosen:
        for i in s.get("images", []):
            images.setdefault(i["image"], {"image": i["image"], "present": image_present(i["image"]),
                                           "hint": i["hint"]})
    return {"images": list(images.values()), "memory_mb": sum(s.get("memory_mb", 0) for s in chosen)}


def missing(needs: dict) -> list:
    """A problem for each image the machine lacks, with the template's hint."""
    return [problem(f"needs.images.{i['image']}", f"the Docker image {i['image']} is missing: {i['hint']}")
            for i in needs["images"] if not i["present"]]
