from .pricing import total


def quote(items):
    return {"total": total(items)}


def quote_many(orders):
    return [quote(order.items) for order in orders]
