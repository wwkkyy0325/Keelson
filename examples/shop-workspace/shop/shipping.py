from .pricing import shipping_band, total


def shipping_quote(items):
    return round(total(items) + shipping_band(1), 2)


def shipping_label(items):
    return f"shipping: {shipping_quote(items):.2f}"
