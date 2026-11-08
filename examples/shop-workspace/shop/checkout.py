from .pricing import total


def checkout(items):
    return total(items)


def checkout_many(orders):
    return [checkout(order.items) for order in orders]
