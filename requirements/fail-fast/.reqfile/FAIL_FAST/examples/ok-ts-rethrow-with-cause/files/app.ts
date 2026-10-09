export async function save(order: Order) {
  try {
    await db.insert(order);
  } catch (e) {
    throw new SaveError("could not save the order", { cause: e });
  }
}
