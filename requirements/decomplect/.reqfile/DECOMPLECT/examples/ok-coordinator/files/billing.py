def bill(customer_id):
    customer = load_customer(customer_id)
    total = price(customer.cart, customer.tier)
    save_invoice(customer_id, total)
    notify(customer, total)
