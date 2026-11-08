def total(items, include_tax=False):
    subtotal = sum(item.price * item.quantity for item in items)
    if include_tax:
        return subtotal * 1.08
    return subtotal
