def bill(customer_id):
    customer = db.fetch_customer(customer_id)
    total = sum(item.price for item in customer.cart)
    if customer.tier == "gold":
        total *= 0.9
    db.save_invoice(customer_id, total)
    mailer.send(customer.email, f"You owe {total}")
