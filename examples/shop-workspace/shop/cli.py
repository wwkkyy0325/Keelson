from .reporting import render


def main(items):
    print(render(items))


def summary(items):
    return render(items)
