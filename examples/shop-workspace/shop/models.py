class Item:
    def __init__(self, name, price, quantity=1, category="general"):
        self.name = name
        self.price = price
        self.quantity = quantity
        self.category = category


class Order:
    def __init__(self, items, customer=None):
        self.items = items
        self.customer = customer

    def add(self, item):
        self.items.append(item)
        return self
