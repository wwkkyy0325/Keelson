from .service import total


def render(items):
    return f"total: {total(items):.2f}"
