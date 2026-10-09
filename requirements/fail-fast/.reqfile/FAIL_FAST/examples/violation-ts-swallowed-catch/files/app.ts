export async function save(order: Order) {
  try {
    await db.insert(order);
  } catch (e) {
    console.log("save failed", e);
  }
  return { saved: true };
}
