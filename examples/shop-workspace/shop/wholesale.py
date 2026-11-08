from .pricing import total


def wholesale_total(items):
    return total(items)


def wholesale_quote(items):
    return {"total": wholesale_total(items)}
