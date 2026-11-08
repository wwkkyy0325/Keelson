"""Pricing helpers for the shop example."""

TAX_RATE = 0.08
CURRENCY = "USD"


def round_money(value):
    return round(value + 1e-9, 2)


def line_total(item):
    return round_money(item.price * item.quantity)


def subtotal(items):
    return round_money(sum(line_total(item) for item in items))


def total(items, include_tax=False):
    value = subtotal(items)
    if include_tax:
        return round_money(value * (1 + TAX_RATE))
    return value


def tax_amount(items):
    return round_money(total(items, include_tax=True) - total(items))


def category_total(items, category):
    selected = [item for item in items if item.category == category]
    return subtotal(selected)


def price_breakdown(items, include_tax=False):
    value = total(items, include_tax=include_tax)
    net = subtotal(items)
    return {"subtotal": net, "tax": round_money(value - net), "total": value}


def format_amount(value):
    return f"{CURRENCY} {value:.2f}"


def bulk_price(price, quantity):
    if quantity >= 50:
        return round_money(price * 0.75)
    if quantity >= 20:
        return round_money(price * 0.85)
    if quantity >= 10:
        return round_money(price * 0.92)
    return round_money(price)


def member_price(price, years):
    if years >= 5:
        return round_money(price * 0.88)
    if years >= 2:
        return round_money(price * 0.94)
    return round_money(price)


def tax_band(country):
    bands = {"US": 0.08, "DE": 0.19, "FR": 0.20, "JP": 0.10}
    return bands.get(country, 0.0)


def shipping_band(weight):
    if weight <= 1:
        return 5.0
    if weight <= 5:
        return 9.0
    if weight <= 20:
        return 15.0
    return 25.0


def quote_line(item):
    value = line_total(item)
    return {"name": item.name, "total": value, "display": format_amount(value)}


def summary_rows(items):
    return [quote_line(item) for item in items]


def tax_summary(items):
    return {
        "net": subtotal(items),
        "tax": tax_amount(items),
        "gross": total(items, include_tax=True),
    }


def discount_preview(items, rate):
    return round_money(subtotal(items) * (1 - rate))


def packaging_fee(quantity):
    if quantity >= 50:
        return 0.0
    if quantity >= 10:
        return 1.5
    return 2.5


def handling_fee(weight):
    if weight <= 1:
        return 0.0
    if weight <= 10:
        return 2.0
    return 4.0


def zone_for(country):
    zones = {"US": 1, "CA": 1, "DE": 2, "FR": 2, "JP": 3}
    return zones.get(country, 3)


def estimate_weight(items, unit_weight=0.1):
    return round(sum(item.quantity for item in items) * unit_weight, 2)
