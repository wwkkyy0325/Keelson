from .pricing import total


def stock_value(items):
    return total(items)


def low_stock(items, threshold=2):
    return [item.name for item in items if item.quantity <= threshold]
