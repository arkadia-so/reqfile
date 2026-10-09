def price(cart, tier):
    total = sum(item.price for item in cart)
    if tier == "gold":
        total *= 0.9
    return round(total, 2)
