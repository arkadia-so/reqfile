export async function userAge(url: string): Promise<number> {
  const response = await fetch(url);
  return (await response.json()).age;
}
