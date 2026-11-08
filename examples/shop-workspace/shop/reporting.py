from .pricing import format_amount, price_breakdown, total


def render(items):
    return f"total: {total(items):.2f}"


def render_with_tax(items):
    return f"total: {total(items, include_tax=True):.2f}"


def invoice(items, include_tax=False):
    value = total(items, include_tax=include_tax)
    return {"total": value, "display": format_amount(value)}


def breakdown(items, include_tax=False):
    return price_breakdown(items, include_tax=include_tax)
